use std::collections::HashSet;

use serde::Serialize;

pub const OPEN_COMMAND_ID: &str = "session:open-four-fixtures";
pub const SWITCH_COMMAND_ID: &str = "session:switch-four-fixtures";
pub const EDIT_COMMAND_ID: &str = "session:edit-dense-rectangle";
pub const CLOSE_COMMAND_ID: &str = "session:close-three-and-recover";
pub const DENSE_RECTANGLE_ID: &str = "comparison:rectangle:session:1";

pub const FIXTURE_IDS: [&str; 4] = [
    "bp-single-page-v1",
    "nasa-apollo-summary-526-v1",
    "bp-engineering-sheet-v1",
    "bp-annotation-density-v1",
];
pub const SWITCH_SEQUENCE: [&str; 4] = [
    "nasa-apollo-summary-526-v1",
    "bp-single-page-v1",
    "bp-engineering-sheet-v1",
    "bp-annotation-density-v1",
];
pub const CLOSE_SEQUENCE: [&str; 3] = [
    "bp-single-page-v1",
    "bp-engineering-sheet-v1",
    "nasa-apollo-summary-526-v1",
];
pub const DENSE_FIXTURE_ID: &str = "bp-annotation-density-v1";

pub const OPEN_MILESTONES: [&str; 6] = [
    "application-process-id-recorded",
    "four-documents-opened",
    "tab-order-exact",
    "document-identities-distinct",
    "current-raster-after-each-open",
    "aggregate-resource-observations-complete",
];
pub const SWITCH_MILESTONES: [&str; 6] = [
    "application-process-id-stable",
    "trusted-native-input-complete",
    "switch-sequence-exact",
    "per-document-state-isolated",
    "current-raster-after-each-switch",
    "aggregate-resource-observations-complete",
];
pub const EDIT_MILESTONES: [&str; 7] = [
    "application-process-id-stable",
    "trusted-native-input-complete",
    "dense-rectangle-created-once",
    "dense-rectangle-property-gesture-observed",
    "dense-document-dirty",
    "other-document-states-unchanged",
    "thumbnail-current",
];
pub const CLOSE_MILESTONES: [&str; 8] = [
    "application-process-id-stable",
    "close-three-sequence-exact",
    "closed-document-resources-released",
    "memory-recovery-recorded",
    "one-document-remains",
    "dense-document-active",
    "dense-rectangle-property-current",
    "interactive-document-shell",
];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DocumentResourceObservation {
    pub fixture_id: String,
    pub document_id: u64,
    pub page_count: usize,
    pub decoded_page_bytes: usize,
    pub current_raster: bool,
    pub annotation_count: usize,
    pub dense_page_annotation_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClosedDocumentObservation {
    pub fixture_id: String,
    pub document_id: u64,
    pub released_decoded_page_bytes: usize,
    pub document_removed: bool,
    pub render_requests_removed: bool,
    pub annotation_state_removed: bool,
}

pub fn validate_open_observations(
    observations: &[DocumentResourceObservation],
) -> Result<(), String> {
    if observations.len() != FIXTURE_IDS.len() {
        return Err("multi-document open must observe exactly four documents".into());
    }
    let actual = observations
        .iter()
        .map(|observation| observation.fixture_id.as_str())
        .collect::<Vec<_>>();
    if actual != FIXTURE_IDS {
        return Err("multi-document tab order does not match the frozen fixture order".into());
    }
    let identities = observations
        .iter()
        .map(|observation| observation.document_id)
        .collect::<HashSet<_>>();
    if identities.len() != FIXTURE_IDS.len() || identities.contains(&0) {
        return Err("multi-document identities are not four distinct nonzero IDs".into());
    }
    if observations.iter().any(|observation| {
        observation.page_count == 0
            || observation.decoded_page_bytes == 0
            || !observation.current_raster
    }) {
        return Err("every opened document needs a current raster and decoded resources".into());
    }
    for observation in observations {
        let expected = if observation.fixture_id == DENSE_FIXTURE_ID {
            (1_000, 100)
        } else {
            (0, 0)
        };
        if (
            observation.annotation_count,
            observation.dense_page_annotation_count,
        ) != expected
        {
            return Err(format!(
                "{} annotation load is not exact: observed {}/{}",
                observation.fixture_id,
                observation.annotation_count,
                observation.dense_page_annotation_count
            ));
        }
    }
    Ok(())
}

pub fn validate_switch_observations(
    fixtures: &[String],
    current_raster: &[bool],
    document_state_isolated: bool,
) -> Result<(), String> {
    let actual = fixtures.iter().map(String::as_str).collect::<Vec<_>>();
    if actual != SWITCH_SEQUENCE {
        return Err("native document switch order does not match the frozen sequence".into());
    }
    if current_raster.len() != SWITCH_SEQUENCE.len()
        || current_raster.iter().any(|current| !current)
    {
        return Err("each native switch needs a current document raster".into());
    }
    if !document_state_isolated {
        return Err("document state changed outside the active dense document".into());
    }
    Ok(())
}

pub fn validate_closed_observations(
    observations: &[ClosedDocumentObservation],
    remaining_fixture_id: &str,
    remaining_document_count: usize,
    remaining_active: bool,
    dense_property_width_pt: f64,
) -> Result<(), String> {
    let actual = observations
        .iter()
        .map(|observation| observation.fixture_id.as_str())
        .collect::<Vec<_>>();
    if actual != CLOSE_SEQUENCE {
        return Err("document close order does not match the frozen sequence".into());
    }
    if observations.iter().any(|observation| {
        observation.released_decoded_page_bytes == 0
            || !observation.document_removed
            || !observation.render_requests_removed
            || !observation.annotation_state_removed
    }) {
        return Err("closed document resources were not completely released".into());
    }
    if remaining_fixture_id != DENSE_FIXTURE_ID
        || remaining_document_count != 1
        || !remaining_active
    {
        return Err("the dense fixture is not the sole active remaining document".into());
    }
    if (dense_property_width_pt - 4.0).abs() > f64::EPSILON {
        return Err("the dense rectangle 4 pt property edit is not current".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMBEDDED_V5_WORKLOAD: &str =
        include_str!("../../performance/comparison-workload-v5.materialized.json");

    fn opened() -> Vec<DocumentResourceObservation> {
        FIXTURE_IDS
            .iter()
            .enumerate()
            .map(|(index, fixture_id)| DocumentResourceObservation {
                fixture_id: (*fixture_id).into(),
                document_id: index as u64 + 1,
                page_count: index + 1,
                decoded_page_bytes: 4096,
                current_raster: true,
                annotation_count: usize::from(*fixture_id == DENSE_FIXTURE_ID) * 1_000,
                dense_page_annotation_count: usize::from(*fixture_id == DENSE_FIXTURE_ID) * 100,
            })
            .collect()
    }

    #[test]
    fn open_receipt_requires_exact_order_identity_and_resources() {
        assert_eq!(validate_open_observations(&opened()), Ok(()));
        let mut drift = opened();
        drift.swap(0, 1);
        assert!(validate_open_observations(&drift).is_err());
        let mut missing = opened();
        missing[2].decoded_page_bytes = 0;
        assert!(validate_open_observations(&missing).is_err());
    }

    #[test]
    fn switch_receipt_requires_exact_native_order_and_current_rasters() {
        let fixtures = SWITCH_SEQUENCE.map(str::to_owned);
        assert_eq!(
            validate_switch_observations(&fixtures, &[true; 4], true),
            Ok(())
        );
        assert!(validate_switch_observations(&fixtures, &[true, false, true, true], true).is_err());
    }

    #[test]
    fn close_receipt_requires_three_releases_and_dense_active() {
        let observations = CLOSE_SEQUENCE
            .iter()
            .enumerate()
            .map(|(index, fixture_id)| ClosedDocumentObservation {
                fixture_id: (*fixture_id).into(),
                document_id: index as u64 + 1,
                released_decoded_page_bytes: 4096,
                document_removed: true,
                render_requests_removed: true,
                annotation_state_removed: true,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            validate_closed_observations(&observations, DENSE_FIXTURE_ID, 1, true, 4.0,),
            Ok(())
        );
        assert!(
            validate_closed_observations(&observations, DENSE_FIXTURE_ID, 2, true, 4.0).is_err()
        );
    }

    #[test]
    fn native_milestones_match_the_frozen_v5_workload() {
        let workload: serde_json::Value = serde_json::from_str(EMBEDDED_V5_WORKLOAD).unwrap();
        let journey = workload["journeys"]
            .as_array()
            .unwrap()
            .iter()
            .find(|journey| journey["id"] == "multi-document-session-v1")
            .unwrap();
        let commands = journey["commands"].as_array().unwrap();

        for (command_id, expected) in [
            (OPEN_COMMAND_ID, OPEN_MILESTONES.as_slice()),
            (SWITCH_COMMAND_ID, SWITCH_MILESTONES.as_slice()),
            (EDIT_COMMAND_ID, EDIT_MILESTONES.as_slice()),
            (CLOSE_COMMAND_ID, CLOSE_MILESTONES.as_slice()),
        ] {
            let command = commands
                .iter()
                .find(|command| command["id"] == command_id)
                .unwrap();
            let actual = command["expected_milestones"]
                .as_array()
                .unwrap()
                .iter()
                .map(|milestone| milestone.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{command_id} milestone drift");
        }
    }
}
