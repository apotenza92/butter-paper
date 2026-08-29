#define _POSIX_C_SOURCE 200809L
#include <X11/Xlib.h>
#include <X11/keysym.h>
#include <X11/extensions/Xpresent.h>
#include <X11/extensions/presenttokens.h>
#include <errno.h>
#include <math.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

extern int XTestFakeMotionEvent(Display *, int, int, int, unsigned long);
extern int XTestFakeButtonEvent(Display *, unsigned int, int, unsigned long);
extern int XTestFakeKeyEvent(Display *, unsigned int, int, unsigned long);

typedef struct {
  const char *name;
  const char *token;
  long sequence;
  long event_count;
  double input_ms;
  double action_completed_ms;
  double *sample_ms;
  long sample_count;
  Window input_window;
  Window verified_input_window;
} ActionReceipt;

static void fail(const char *message) {
  fprintf(stderr, "native-x11-present-observer: %s\n", message);
  exit(2);
}

static long parse_long(const char *text, const char *label) {
  char *end = NULL;
  errno = 0;
  long value = strtol(text, &end, 10);
  if (errno != 0 || end == text || *end != '\0') fail(label);
  return value;
}

static double monotonic_ms(void) {
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) fail("clock_gettime failed");
  return (double)now.tv_sec * 1000.0 + (double)now.tv_nsec / 1000000.0;
}

static struct timespec add_ns(struct timespec start, int64_t nanoseconds) {
  start.tv_sec += nanoseconds / 1000000000LL;
  start.tv_nsec += nanoseconds % 1000000000LL;
  if (start.tv_nsec >= 1000000000L) {
    start.tv_sec += 1;
    start.tv_nsec -= 1000000000L;
  }
  return start;
}

static void wait_until(struct timespec deadline) {
  int result;
  do {
    result = clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &deadline, NULL);
  } while (result == EINTR);
  if (result != 0) fail("clock_nanosleep failed");
}

static void require_token(const char *token) {
  size_t length = strlen(token);
  if (length < 1 || length > 96) fail("action token length is invalid");
  for (size_t index = 0; index < length; index += 1) {
    char value = token[index];
    if (!((value >= 'a' && value <= 'z') ||
          (value >= 'A' && value <= 'Z') ||
          (value >= '0' && value <= '9') || value == '-' || value == '_' ||
          value == ':' || value == '.'))
      fail("action token contains an invalid character");
  }
}

static void drain_events(Display *display) {
  while (XPending(display) > 0) {
    XEvent event;
    XNextEvent(display, &event);
  }
}

static int wait_for_present_complete(Display *display, int present_opcode,
                                     XID event_id, Window window,
                                     double not_before_ms, double deadline_ms,
                                     XPresentCompleteNotifyEvent *observed,
                                     double *received_ms) {
  for (;;) {
    while (XPending(display) > 0) {
      XEvent event;
      XNextEvent(display, &event);
      if (event.type != GenericEvent ||
          event.xcookie.extension != present_opcode ||
          event.xcookie.evtype != PresentCompleteNotify)
        continue;
      if (!XGetEventData(display, &event.xcookie))
        fail("XGetEventData failed for PresentCompleteNotify");
      XPresentCompleteNotifyEvent *complete = event.xcookie.data;
      double now_ms = monotonic_ms();
      int matches = complete != NULL && complete->eid == event_id &&
                    complete->window == window &&
                    complete->kind == PresentCompleteKindPixmap &&
                    now_ms > not_before_ms;
      if (matches) {
        *observed = *complete;
        *received_ms = now_ms;
      }
      XFreeEventData(display, &event.xcookie);
      if (matches) return 1;
    }
    double remaining_ms = deadline_ms - monotonic_ms();
    if (remaining_ms <= 0.0) return 0;
    int timeout_ms = (int)ceil(remaining_ms);
    struct pollfd descriptor = {
        .fd = ConnectionNumber(display), .events = POLLIN, .revents = 0};
    int result;
    do {
      result = poll(&descriptor, 1, timeout_ms);
    } while (result < 0 && errno == EINTR);
    if (result < 0) fail("poll failed while waiting for PresentCompleteNotify");
    if (result == 0) return 0;
    if ((descriptor.revents & (POLLERR | POLLHUP | POLLNVAL)) != 0)
      fail("X11 connection failed while waiting for PresentCompleteNotify");
  }
}

static void inject_click(Display *display, int screen, int argc, char **argv,
                         ActionReceipt *receipt) {
  if (argc != 10)
    fail("usage: sample window timeout_ms token sequence click x y button");
  long x = parse_long(argv[7], "invalid click x");
  long y = parse_long(argv[8], "invalid click y");
  long button = parse_long(argv[9], "invalid click button");
  if (button < 1 || button > 7) fail("click button must be 1..7");
  if (!XTestFakeMotionEvent(display, screen, (int)x, (int)y, CurrentTime))
    fail("click target motion failed");
  XSync(display, False);
  drain_events(display);
  receipt->input_ms = monotonic_ms();
  if (!XTestFakeButtonEvent(display, (unsigned int)button, True, CurrentTime) ||
      !XTestFakeButtonEvent(display, (unsigned int)button, False, CurrentTime))
    fail("click injection failed");
  XFlush(display);
  receipt->action_completed_ms = monotonic_ms();
  receipt->event_count = 1;
}

static void inject_key(Display *display, int argc, char **argv,
                       ActionReceipt *receipt) {
  if (argc != 9)
    fail("usage: sample present_window timeout_ms token sequence key input_window keysym");
  long input_window_id = parse_long(argv[7], "invalid key input window");
  if (input_window_id <= 0) fail("key input window must be positive");
  Window focused_window = None;
  int revert_to = RevertToNone;
  XGetInputFocus(display, &focused_window, &revert_to);
  Window verified_window = (Window)input_window_id;
  Window current = focused_window;
  int focus_matches = current == verified_window;
  for (int depth = 0; !focus_matches && depth < 64; depth += 1) {
    Window root = None;
    Window parent = None;
    Window *children = NULL;
    unsigned int child_count = 0;
    if (current == None || current == PointerRoot ||
        !XQueryTree(display, current, &root, &parent, &children, &child_count))
      break;
    if (children != NULL) XFree(children);
    if (parent == None || parent == current) break;
    current = parent;
    focus_matches = current == verified_window;
  }
  if (focused_window == None || focused_window == PointerRoot || !focus_matches)
    fail("XGetInputFocus does not match the verified key input window");
  receipt->input_window = focused_window;
  receipt->verified_input_window = verified_window;
  KeySym keysym = XStringToKeysym(argv[8]);
  if (keysym == NoSymbol) fail("key action has an unknown keysym");
  KeyCode keycode = XKeysymToKeycode(display, keysym);
  if (keycode == 0) fail("key action has no keycode on this display");
  XSync(display, False);
  drain_events(display);
  receipt->input_ms = monotonic_ms();
  if (!XTestFakeKeyEvent(display, keycode, True, CurrentTime) ||
      !XTestFakeKeyEvent(display, keycode, False, CurrentTime))
    fail("key injection failed");
  XFlush(display);
  receipt->action_completed_ms = monotonic_ms();
  receipt->event_count = 1;
}

static void inject_pointer(Display *display, int screen, int argc, char **argv,
                           ActionReceipt *receipt) {
  if (argc < 14)
    fail("usage: sample window timeout_ms token sequence pointer duration_ms count button x y ...");
  long duration_ms = parse_long(argv[7], "invalid pointer duration");
  long count = parse_long(argv[8], "invalid pointer count");
  long button = parse_long(argv[9], "invalid pointer button");
  if (duration_ms <= 0 || count < 2 || button < 1 || button > 3 ||
      argc != 10 + count * 2)
    fail("invalid pointer schedule");
  receipt->sample_ms = calloc((size_t)count, sizeof(double));
  if (receipt->sample_ms == NULL) fail("pointer timestamp allocation failed");
  receipt->sample_count = count;
  int x = (int)parse_long(argv[10], "invalid pointer x");
  int y = (int)parse_long(argv[11], "invalid pointer y");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime) ||
      !XTestFakeButtonEvent(display, (unsigned int)button, True, CurrentTime))
    fail("initial pointer injection failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0)
    fail("clock_gettime failed");
  receipt->sample_ms[0] = monotonic_ms();
  for (long index = 1; index < count; index += 1) {
    int64_t offset_ns =
        (int64_t)duration_ms * 1000000LL * index / (count - 1);
    wait_until(add_ns(start, offset_ns));
    x = (int)parse_long(argv[10 + index * 2], "invalid pointer x");
    y = (int)parse_long(argv[11 + index * 2], "invalid pointer y");
    if (index == count - 1) {
      XSync(display, False);
      drain_events(display);
      receipt->input_ms = monotonic_ms();
    }
    if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime))
      fail("pointer motion injection failed");
    XFlush(display);
    receipt->sample_ms[index] = monotonic_ms();
  }
  if (!XTestFakeButtonEvent(display, (unsigned int)button, False, CurrentTime))
    fail("pointer release failed");
  XFlush(display);
  receipt->action_completed_ms = monotonic_ms();
  receipt->event_count = count + 1;
}

static void inject_wheel(Display *display, int screen, int argc, char **argv,
                         ActionReceipt *receipt) {
  if (argc != 14)
    fail("usage: sample window timeout_ms token sequence wheel forward_ms pause_ms reverse_ms rate_hz reverse_notches x y");
  long forward_ms = parse_long(argv[7], "invalid forward duration");
  long pause_ms = parse_long(argv[8], "invalid pause duration");
  long reverse_ms = parse_long(argv[9], "invalid reverse duration");
  long rate_hz = parse_long(argv[10], "invalid wheel rate");
  long reverse_notches = parse_long(argv[11], "invalid reverse notches");
  int x = (int)parse_long(argv[12], "invalid wheel x");
  int y = (int)parse_long(argv[13], "invalid wheel y");
  long forward_count = forward_ms * rate_hz / 1000;
  long reverse_count = reverse_ms * rate_hz / 1000;
  if (forward_ms <= 0 || pause_ms < 0 || reverse_ms <= 0 || rate_hz <= 0 ||
      reverse_notches < 1 || forward_count < 1 || reverse_count < 1 ||
      forward_count * 1000 != forward_ms * rate_hz ||
      reverse_count * 1000 != reverse_ms * rate_hz)
    fail("invalid wheel schedule");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime))
    fail("wheel target motion failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0)
    fail("clock_gettime failed");
  for (long index = 0; index < forward_count; index += 1) {
    wait_until(add_ns(start, (int64_t)forward_ms * 1000000LL * (index + 1) /
                                    forward_count));
    if (!XTestFakeButtonEvent(display, 5, True, CurrentTime) ||
        !XTestFakeButtonEvent(display, 5, False, CurrentTime))
      fail("forward wheel injection failed");
    XFlush(display);
  }
  int64_t reverse_start_ns = (int64_t)(forward_ms + pause_ms) * 1000000LL;
  for (long index = 0; index < reverse_count; index += 1) {
    wait_until(add_ns(start, reverse_start_ns +
                                (int64_t)reverse_ms * 1000000LL * (index + 1) /
                                    reverse_count));
    if (index == reverse_count - 1) {
      XSync(display, False);
      drain_events(display);
      receipt->input_ms = monotonic_ms();
    }
    for (long notch = 0; notch < reverse_notches; notch += 1) {
      if (!XTestFakeButtonEvent(display, 4, True, CurrentTime) ||
          !XTestFakeButtonEvent(display, 4, False, CurrentTime))
        fail("reverse wheel injection failed");
    }
    XFlush(display);
  }
  receipt->action_completed_ms = monotonic_ms();
  receipt->event_count = forward_count + reverse_count * reverse_notches;
}

static void inject_action(Display *display, int screen, int argc, char **argv,
                          ActionReceipt *receipt) {
  if (argc < 9)
    fail("usage: sample window timeout_ms token sequence action ...");
  receipt->token = argv[4];
  require_token(receipt->token);
  receipt->sequence = parse_long(argv[5], "invalid action sequence");
  if (receipt->sequence < 0) fail("action sequence must not be negative");
  receipt->name = argv[6];
  if (strcmp(receipt->name, "click") == 0)
    inject_click(display, screen, argc, argv, receipt);
  else if (strcmp(receipt->name, "key") == 0)
    inject_key(display, argc, argv, receipt);
  else if (strcmp(receipt->name, "pointer") == 0)
    inject_pointer(display, screen, argc, argv, receipt);
  else if (strcmp(receipt->name, "wheel") == 0)
    inject_wheel(display, screen, argc, argv, receipt);
  else
    fail("unsupported causal action; expected click, key, pointer, or wheel");
}

static int run_sample(Display *display, int screen, int argc, char **argv) {
  if (argc < 9) fail("usage: sample window timeout_ms token sequence action ...");
  long window_id = parse_long(argv[2], "invalid target window id");
  long timeout_ms = parse_long(argv[3], "invalid timeout");
  if (window_id <= 0 || timeout_ms < 1 || timeout_ms > 60000)
    fail("sample target or timeout is outside the supported range");
  Window window = (Window)window_id;
  XWindowAttributes attributes;
  if (!XGetWindowAttributes(display, window, &attributes))
    fail("target window does not exist");
  if (attributes.map_state != IsViewable) fail("target window is not viewable");

  int present_opcode = 0;
  int present_event_base = 0;
  int present_error_base = 0;
  if (!XPresentQueryExtension(display, &present_opcode, &present_event_base,
                              &present_error_base))
    fail("X11 Present extension is unavailable");
  int present_major = 0;
  int present_minor = 0;
  if (!XPresentQueryVersion(display, &present_major, &present_minor))
    fail("X11 Present version query failed");
  XID event_id = XPresentSelectInput(display, window, PresentCompleteNotifyMask);
  if (event_id == 0) fail("XPresentSelectInput failed");
  XSync(display, False);
  drain_events(display);

  ActionReceipt action = {0};
  action.input_window = window;
  action.verified_input_window = window;
  inject_action(display, screen, argc, argv, &action);
  if (!(action.input_ms > 0.0) ||
      action.action_completed_ms < action.input_ms)
    fail("causal action timestamps are invalid");

  XPresentCompleteNotifyEvent complete;
  memset(&complete, 0, sizeof(complete));
  double completed_ms = 0.0;
  int received = wait_for_present_complete(
      display, present_opcode, event_id, window, action.action_completed_ms,
      action.input_ms + (double)timeout_ms, &complete, &completed_ms);
  XPresentFreeInput(display, window, event_id);
  XSync(display, False);
  if (!received) fail("timed out waiting for causal PresentCompleteNotify");
  double latency_ms = completed_ms - action.input_ms;
  if (!(latency_ms > 0.0) || !isfinite(latency_ms))
    fail("Present completion latency is not finite and positive");

  const char *input_target_relation =
      action.input_window == window ? "same-window" : "verified-distinct-window";
  printf("{\"schema_version\":2,\"observer\":\"native-x11-present-observer-v1\","
         "\"observer_pid\":%ld,\"window_id\":\"%lu\","
         "\"input_window_id\":\"%lu\",\"verified_input_window_id\":\"%lu\","
         "\"present_window_id\":\"%lu\","
         "\"input_target_relation\":\"%s\","
         "\"input_api\":\"XTEST\",\"action\":\"%s\","
         "\"action_token\":\"%s\",\"action_sequence\":%ld,"
         "\"action_event_count\":%ld,\"action_position\":\"terminal\","
         "\"correlation_method\":\"observer-owned-terminal-XTEST-action-after-event-drain\","
         "\"input_clock\":\"CLOCK_MONOTONIC\","
         "\"completion_clock\":\"CLOCK_MONOTONIC\","
         "\"completion_signal\":\"X11-PresentCompleteNotify\","
         "\"physical_scanout_observed\":false,"
         "\"target_viewable_before_action\":true,"
         "\"target_width\":%d,\"target_height\":%d,"
         "\"present_extension_major\":%d,\"present_extension_minor\":%d,"
         "\"input_monotonic_ms\":%.6f,"
         "\"action_completed_monotonic_ms\":%.6f,"
         "\"present_complete_received_monotonic_ms\":%.6f,"
         "\"input_to_present_complete_ms\":%.6f,"
         "\"present_event_id\":\"%lu\",\"present_serial\":%u,"
         "\"present_ust\":%llu,\"present_msc\":%llu,"
         "\"present_kind\":%u,\"present_mode\":%u,\"injected_samples\":[",
         (long)getpid(), (unsigned long)window,
         (unsigned long)action.input_window,
         (unsigned long)action.verified_input_window, (unsigned long)window,
         input_target_relation, action.name, action.token,
         action.sequence, action.event_count, attributes.width, attributes.height,
         present_major, present_minor, action.input_ms,
         action.action_completed_ms, completed_ms, latency_ms,
         (unsigned long)event_id, complete.serial_number,
         (unsigned long long)complete.ust, (unsigned long long)complete.msc,
         (unsigned int)complete.kind, (unsigned int)complete.mode);
  for (long index = 0; index < action.sample_count; index += 1) {
    if (index > 0) printf(",");
    printf("{\"sample_index\":%ld,\"observed_monotonic_ms\":%.6f,\"action\":\"%s\"}",
           index, action.sample_ms[index], index == 0 ? "down" : "move");
  }
  printf("]}\n");
  fflush(stdout);
  free(action.sample_ms);
  return 0;
}

static int run_self_test(int argc) {
  if (argc != 2) fail("usage: self-test");
  double first = monotonic_ms();
  double second = monotonic_ms();
  if (!isfinite(first) || !isfinite(second) || second < first)
    fail("CLOCK_MONOTONIC self-test failed");
  require_token("component:command_1");
  printf("self-test-ok\n");
  return 0;
}

int main(int argc, char **argv) {
  if (argc < 2) fail("missing observer mode");
  if (strcmp(argv[1], "self-test") == 0) return run_self_test(argc);
  if (strcmp(argv[1], "sample") != 0) fail("unsupported observer mode");
  Display *display = XOpenDisplay(NULL);
  if (display == NULL) fail("cannot open DISPLAY");
  int result = run_sample(display, DefaultScreen(display), argc, argv);
  XCloseDisplay(display);
  return result;
}
