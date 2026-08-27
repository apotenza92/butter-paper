use butter_paper_gpui_gallery::pdf_worker::{
    CancellationRegistry, ClipRect, DocumentInfo, FileSurfaceStore, JobId, JsonLineReceiver,
    JsonLineSender, PageGeometry, PdfBackend, RenderRequest, RequestId, Rotation, SessionId,
    SourceHandleId, SurfaceDescriptor, SurfaceFormat, SurfaceId, SurfaceLimits, WorkerError,
    WorkerErrorCode, WorkerRequest, WorkerResponse, WorkerState, run_worker_protocol,
};
use std::fs;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Default)]
struct FakeBackend;

#[derive(Debug)]
struct FakeDocument;

impl PdfBackend for FakeBackend {
    type Document = FakeDocument;

    fn open(
        &mut self,
        _source: SourceHandleId,
        password: Option<&str>,
    ) -> Result<(Self::Document, DocumentInfo), WorkerError> {
        if password == Some("wrong") {
            return Err(WorkerError::new(WorkerErrorCode::BadPassword));
        }
        Ok((
            FakeDocument,
            DocumentInfo {
                page_count: 1,
                repaired: false,
            },
        ))
    }

    fn page_geometry(
        &mut self,
        _document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageGeometry, WorkerError> {
        if page_index != 0 {
            return Err(WorkerError::new(WorkerErrorCode::PageError));
        }
        Ok(PageGeometry {
            media_box: [0.0, 0.0, 612.0, 792.0],
            crop_box: [0.0, 0.0, 612.0, 792.0],
            rotation: Rotation::Degrees0,
            display_width_points: 612.0,
            display_height_points: 792.0,
            user_unit: 1.0,
        })
    }

    fn render_crop(
        &mut self,
        _document: &mut Self::Document,
        request: &RenderRequest,
        output: &mut [u8],
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), WorkerError> {
        if request.page_index == 99 {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !cancelled.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::yield_now();
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(WorkerError::new(WorkerErrorCode::Cancelled));
            }
        }
        output.fill(0x7f);
        Ok(())
    }

    fn close(&mut self, _document: Self::Document) {}
}

fn surface(id: u64, width: u32, height: u32) -> SurfaceDescriptor {
    SurfaceDescriptor {
        surface_id: SurfaceId(id),
        width,
        height,
        stride: width * 4,
        byte_len: u64::from(width) * u64::from(height) * 4,
        format: SurfaceFormat::Bgra8Premultiplied,
    }
}

#[test]
fn protocol_round_trips_without_pdfium_types_or_encoded_images() {
    let request = WorkerRequest::RenderCrop {
        request_id: RequestId(4),
        render: RenderRequest {
            job_id: JobId(9),
            session_id: SessionId(2),
            page_index: 0,
            include_pdf_annotations: false,
            transform: [2.0, 0.0, 0.0, 2.0, -10.0, -20.0],
            clip: ClipRect {
                x: 0,
                y: 0,
                width: 64,
                height: 32,
            },
            surface: surface(7, 64, 32),
        },
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"type\":\"render_crop\""));
    assert!(json.contains("bgra8_premultiplied"));
    assert!(!json.contains("png"));
    assert!(!json.contains("base64"));
    assert_eq!(
        serde_json::from_str::<WorkerRequest>(&json).unwrap(),
        request
    );
}

#[test]
fn surface_validation_rejects_overflow_and_limit_violations() {
    let limits = SurfaceLimits {
        max_dimension: 8_192,
        max_pixels: 32 * 1024 * 1024,
        max_bytes: 128 * 1024 * 1024,
    };
    assert_eq!(surface(1, 64, 32).validate(limits), Ok(64 * 32 * 4));

    let mut overflow = surface(2, 1, 1);
    overflow.width = u32::MAX;
    overflow.height = u32::MAX;
    overflow.stride = u32::MAX;
    overflow.byte_len = u64::MAX;
    assert_eq!(
        overflow.validate(limits).unwrap_err().code,
        WorkerErrorCode::LimitExceeded
    );
}

#[test]
fn state_opens_queries_renders_and_closes_a_document() {
    let temp = tempfile::tempdir().unwrap();
    let descriptor = surface(17, 8, 4);
    let _mapped_surface = FileSurfaceStore::create_surface(temp.path(), &descriptor).unwrap();
    let registry = CancellationRegistry::default();
    let mut state = WorkerState::new(
        FakeBackend,
        FileSurfaceStore::new(temp.path()),
        SurfaceLimits::default(),
        registry,
    );

    assert!(matches!(
        state.handle(WorkerRequest::Open {
            request_id: RequestId(1),
            session_id: SessionId(3),
            source_handle_id: SourceHandleId(11),
            password: None,
        }),
        WorkerResponse::Opened { .. }
    ));
    assert!(matches!(
        state.handle(WorkerRequest::PageGeometry {
            request_id: RequestId(2),
            session_id: SessionId(3),
            page_index: 0,
        }),
        WorkerResponse::PageGeometry { .. }
    ));
    assert!(matches!(
        state.handle(WorkerRequest::RenderCrop {
            request_id: RequestId(3),
            render: RenderRequest {
                job_id: JobId(8),
                session_id: SessionId(3),
                page_index: 0,
                include_pdf_annotations: false,
                transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                clip: ClipRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 4
                },
                surface: descriptor.clone(),
            },
        }),
        WorkerResponse::Rendered { .. }
    ));
    assert_eq!(
        fs::read(FileSurfaceStore::surface_path(temp.path(), SurfaceId(17))).unwrap(),
        vec![0x7f; 128]
    );
    assert!(matches!(
        state.handle(WorkerRequest::Close {
            request_id: RequestId(4),
            session_id: SessionId(3),
        }),
        WorkerResponse::Closed { .. }
    ));
}

#[test]
fn cancellation_registry_interrupts_an_in_flight_render() {
    let registry = CancellationRegistry::default();
    let token = registry.register(JobId(44)).unwrap();
    let other_thread = registry.clone();
    let join = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        assert!(other_thread.cancel(JobId(44)));
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    while !token.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    join.join().unwrap();
    assert!(token.load(Ordering::Acquire));
    registry.finish(JobId(44));
}

#[test]
fn cancellation_that_arrives_before_actor_registration_is_retained() {
    let registry = CancellationRegistry::default();
    assert!(!registry.cancel(JobId(45)));
    let token = registry.register(JobId(45)).unwrap();
    assert!(token.load(Ordering::Acquire));
    registry.finish(JobId(45));
}

#[test]
fn render_validation_rejects_non_finite_transforms_and_out_of_bounds_clips() {
    let mut request = RenderRequest {
        job_id: JobId(1),
        session_id: SessionId(1),
        page_index: 0,
        include_pdf_annotations: false,
        transform: [1.0, 0.0, 0.0, 1.0, f32::NAN, 0.0],
        clip: ClipRect {
            x: 0,
            y: 0,
            width: 8,
            height: 4,
        },
        surface: surface(1, 8, 4),
    };
    assert_eq!(
        request.validate(SurfaceLimits::default()).unwrap_err().code,
        WorkerErrorCode::InvalidRequest
    );
    request.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    request.clip.x = 7;
    request.clip.width = 2;
    assert_eq!(
        request.validate(SurfaceLimits::default()).unwrap_err().code,
        WorkerErrorCode::InvalidRequest
    );
}

#[test]
fn protocol_control_thread_cancels_an_in_flight_render() {
    let temp = tempfile::tempdir().unwrap();
    let descriptor = surface(23, 8, 4);
    let _mapped_surface = FileSurfaceStore::create_surface(temp.path(), &descriptor).unwrap();

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let surface_root = temp.path().to_owned();
    let server = std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let registry = CancellationRegistry::default();
        let state = WorkerState::new(
            FakeBackend,
            FileSurfaceStore::new(surface_root),
            SurfaceLimits::default(),
            registry.clone(),
        );
        run_worker_protocol(socket.try_clone().unwrap(), socket, state, registry).unwrap();
    });

    let socket = TcpStream::connect(address).unwrap();
    let shutdown = socket.try_clone().unwrap();
    let sender = JsonLineSender::new(socket.try_clone().unwrap());
    let mut receiver = JsonLineReceiver::new(socket);
    let open = WorkerRequest::Open {
        request_id: RequestId(1),
        session_id: SessionId(5),
        source_handle_id: SourceHandleId(1),
        password: None,
    };
    sender.send(&open).unwrap();
    assert!(matches!(
        receiver.receive().unwrap(),
        WorkerResponse::Opened { .. }
    ));

    let render = WorkerRequest::RenderCrop {
        request_id: RequestId(2),
        render: RenderRequest {
            job_id: JobId(77),
            session_id: SessionId(5),
            page_index: 99,
            include_pdf_annotations: false,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            clip: ClipRect {
                x: 0,
                y: 0,
                width: 8,
                height: 4,
            },
            surface: descriptor,
        },
    };
    let cancel = WorkerRequest::Cancel {
        request_id: RequestId(3),
        job_id: JobId(77),
    };
    sender.send(&render).unwrap();
    let control = sender.clone();
    let cancel_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        control.send(&cancel).unwrap();
    });
    assert!(matches!(
        receiver.receive().unwrap(),
        WorkerResponse::Failed {
            job_id: Some(JobId(77)),
            error: WorkerError {
                code: WorkerErrorCode::Cancelled,
                ..
            },
            ..
        }
    ));
    cancel_thread.join().unwrap();
    assert!(matches!(
        receiver.receive().unwrap(),
        WorkerResponse::Cancelled {
            job_id: JobId(77),
            ..
        }
    ));

    shutdown.shutdown(Shutdown::Write).unwrap();
    server.join().unwrap();
}
