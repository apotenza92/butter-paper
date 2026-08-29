#define _POSIX_C_SOURCE 200809L
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/Xdamage.h>
#include <X11/extensions/Xpresent.h>
#include <X11/extensions/presenttokens.h>
#include <errno.h>
#include <fcntl.h>
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

typedef struct {
  int active;
  Window window;
  XID event_id;
  int opcode;
  int major;
  int minor;
  XWindowAttributes attributes;
  const char *token;
  long sequence;
  double input_ms;
  double action_completed_ms;
} PresentBinding;

typedef struct {
  int active;
  Window window;
  Damage damage;
  int event_base;
  int major;
  int minor;
  XWindowAttributes attributes;
  const char *token;
  long sequence;
  double input_ms;
  double action_completed_ms;
} DamageBinding;

typedef struct {
  uint32_t state[8];
  uint64_t total_bytes;
  unsigned char block[64];
  size_t block_bytes;
} Sha256;

static uint32_t sha256_rotr(uint32_t value, unsigned int bits) {
  return (value >> bits) | (value << (32U - bits));
}

static void sha256_transform(Sha256 *hash, const unsigned char block[64]) {
  static const uint32_t constants[64] = {
    0x428a2f98U, 0x71374491U, 0xb5c0fbcfU, 0xe9b5dba5U,
    0x3956c25bU, 0x59f111f1U, 0x923f82a4U, 0xab1c5ed5U,
    0xd807aa98U, 0x12835b01U, 0x243185beU, 0x550c7dc3U,
    0x72be5d74U, 0x80deb1feU, 0x9bdc06a7U, 0xc19bf174U,
    0xe49b69c1U, 0xefbe4786U, 0x0fc19dc6U, 0x240ca1ccU,
    0x2de92c6fU, 0x4a7484aaU, 0x5cb0a9dcU, 0x76f988daU,
    0x983e5152U, 0xa831c66dU, 0xb00327c8U, 0xbf597fc7U,
    0xc6e00bf3U, 0xd5a79147U, 0x06ca6351U, 0x14292967U,
    0x27b70a85U, 0x2e1b2138U, 0x4d2c6dfcU, 0x53380d13U,
    0x650a7354U, 0x766a0abbU, 0x81c2c92eU, 0x92722c85U,
    0xa2bfe8a1U, 0xa81a664bU, 0xc24b8b70U, 0xc76c51a3U,
    0xd192e819U, 0xd6990624U, 0xf40e3585U, 0x106aa070U,
    0x19a4c116U, 0x1e376c08U, 0x2748774cU, 0x34b0bcb5U,
    0x391c0cb3U, 0x4ed8aa4aU, 0x5b9cca4fU, 0x682e6ff3U,
    0x748f82eeU, 0x78a5636fU, 0x84c87814U, 0x8cc70208U,
    0x90befffaU, 0xa4506cebU, 0xbef9a3f7U, 0xc67178f2U,
  };
  uint32_t words[64];
  for (size_t index = 0; index < 16; index += 1) {
    size_t offset = index * 4U;
    words[index] = ((uint32_t)block[offset] << 24U)
                 | ((uint32_t)block[offset + 1U] << 16U)
                 | ((uint32_t)block[offset + 2U] << 8U)
                 | (uint32_t)block[offset + 3U];
  }
  for (size_t index = 16; index < 64; index += 1) {
    uint32_t left = words[index - 15U];
    uint32_t right = words[index - 2U];
    uint32_t sigma0 = sha256_rotr(left, 7U) ^ sha256_rotr(left, 18U)
                    ^ (left >> 3U);
    uint32_t sigma1 = sha256_rotr(right, 17U) ^ sha256_rotr(right, 19U)
                    ^ (right >> 10U);
    words[index] = words[index - 16U] + sigma0 + words[index - 7U] + sigma1;
  }
  uint32_t a = hash->state[0];
  uint32_t b = hash->state[1];
  uint32_t c = hash->state[2];
  uint32_t d = hash->state[3];
  uint32_t e = hash->state[4];
  uint32_t f = hash->state[5];
  uint32_t g = hash->state[6];
  uint32_t h = hash->state[7];
  for (size_t index = 0; index < 64; index += 1) {
    uint32_t sum1 = sha256_rotr(e, 6U) ^ sha256_rotr(e, 11U)
                  ^ sha256_rotr(e, 25U);
    uint32_t choose = (e & f) ^ ((~e) & g);
    uint32_t temp1 = h + sum1 + choose + constants[index] + words[index];
    uint32_t sum0 = sha256_rotr(a, 2U) ^ sha256_rotr(a, 13U)
                  ^ sha256_rotr(a, 22U);
    uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
    uint32_t temp2 = sum0 + majority;
    h = g;
    g = f;
    f = e;
    e = d + temp1;
    d = c;
    c = b;
    b = a;
    a = temp1 + temp2;
  }
  hash->state[0] += a;
  hash->state[1] += b;
  hash->state[2] += c;
  hash->state[3] += d;
  hash->state[4] += e;
  hash->state[5] += f;
  hash->state[6] += g;
  hash->state[7] += h;
}

static void sha256_init(Sha256 *hash) {
  *hash = (Sha256){
    .state = {0x6a09e667U, 0xbb67ae85U, 0x3c6ef372U, 0xa54ff53aU,
              0x510e527fU, 0x9b05688cU, 0x1f83d9abU, 0x5be0cd19U},
  };
}

static void sha256_update(Sha256 *hash, const unsigned char *bytes,
                          size_t length) {
  hash->total_bytes += length;
  while (length > 0) {
    size_t available = sizeof(hash->block) - hash->block_bytes;
    size_t take = length < available ? length : available;
    memcpy(hash->block + hash->block_bytes, bytes, take);
    hash->block_bytes += take;
    bytes += take;
    length -= take;
    if (hash->block_bytes == sizeof(hash->block)) {
      sha256_transform(hash, hash->block);
      hash->block_bytes = 0;
    }
  }
}

static void sha256_final_hex(Sha256 *hash, char output[65]) {
  uint64_t total_bits = hash->total_bytes * 8U;
  hash->block[hash->block_bytes++] = 0x80U;
  if (hash->block_bytes > 56U) {
    memset(hash->block + hash->block_bytes, 0,
           sizeof(hash->block) - hash->block_bytes);
    sha256_transform(hash, hash->block);
    hash->block_bytes = 0;
  }
  memset(hash->block + hash->block_bytes, 0, 56U - hash->block_bytes);
  for (size_t index = 0; index < 8; index += 1) {
    hash->block[63U - index] = (unsigned char)(total_bits >> (index * 8U));
  }
  sha256_transform(hash, hash->block);
  for (size_t index = 0; index < 8; index += 1) {
    snprintf(output + index * 8U, 9U, "%08x", hash->state[index]);
  }
  output[64] = '\0';
}

static void fail(const char *message) {
  fprintf(stderr, "native-x11-xtest: %s\n", message);
  exit(2);
}

static long parse_long(const char *text, const char *label) {
  char *end = NULL;
  errno = 0;
  long value = strtol(text, &end, 10);
  if (errno != 0 || end == text || *end != '\0') fail(label);
  return value;
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

static double monotonic_ms(void) {
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) fail("clock_gettime failed");
  return (double)now.tv_sec * 1000.0 + (double)now.tv_nsec / 1000000.0;
}

static void drain_x11_events(Display *display) {
  while (XPending(display) > 0) {
    XEvent event;
    XNextEvent(display, &event);
  }
}

static PresentBinding begin_present_binding(Display *display) {
  PresentBinding binding = {0};
  const char *window_text = getenv("BP_X11_PRESENT_WINDOW");
  if (window_text == NULL || window_text[0] == '\0') return binding;
  long window_id = parse_long(window_text, "invalid BP_X11_PRESENT_WINDOW");
  binding.token = getenv("BP_X11_PRESENT_ACTION_TOKEN");
  const char *sequence_text = getenv("BP_X11_PRESENT_ACTION_SEQUENCE");
  if (binding.token == NULL || binding.token[0] == '\0' ||
      sequence_text == NULL)
    fail("present binding token or sequence is missing");
  size_t token_length = strlen(binding.token);
  if (token_length > 96) fail("present binding token is too long");
  for (size_t index = 0; index < token_length; index += 1) {
    char value = binding.token[index];
    if (!((value >= 'a' && value <= 'z') ||
          (value >= 'A' && value <= 'Z') ||
          (value >= '0' && value <= '9') || value == '-' || value == '_' ||
          value == ':' || value == '.'))
      fail("present binding token contains an invalid character");
  }
  binding.sequence = parse_long(sequence_text, "invalid present action sequence");
  if (window_id <= 0 || binding.sequence < 0)
    fail("present binding identity is invalid");
  binding.window = (Window)window_id;
  if (!XGetWindowAttributes(display, binding.window, &binding.attributes) ||
      binding.attributes.map_state != IsViewable)
    fail("present binding target is not viewable");
  int event_base = 0;
  int error_base = 0;
  if (!XPresentQueryExtension(display, &binding.opcode, &event_base,
                              &error_base) ||
      !XPresentQueryVersion(display, &binding.major, &binding.minor))
    fail("X11 Present extension is unavailable");
  binding.event_id = XPresentSelectInput(display, binding.window,
                                         PresentCompleteNotifyMask);
  if (binding.event_id == 0) fail("XPresentSelectInput failed");
  XSync(display, False);
  drain_x11_events(display);
  binding.active = 1;
  return binding;
}

static void mark_present_terminal_input(Display *display,
                                        PresentBinding *binding) {
  if (!binding->active) return;
  XSync(display, False);
  drain_x11_events(display);
  binding->input_ms = monotonic_ms();
}

static void finish_present_binding(Display *display, PresentBinding *binding,
                                   long event_count) {
  if (!binding->active) return;
  binding->action_completed_ms = monotonic_ms();
  double deadline_ms = binding->input_ms + 5000.0;
  XPresentCompleteNotifyEvent observed;
  memset(&observed, 0, sizeof(observed));
  double received_ms = 0.0;
  int found = 0;
  while (!found && monotonic_ms() < deadline_ms) {
    while (XPending(display) > 0) {
      XEvent event;
      XNextEvent(display, &event);
      if (event.type != GenericEvent || event.xcookie.extension != binding->opcode ||
          event.xcookie.evtype != PresentCompleteNotify)
        continue;
      if (!XGetEventData(display, &event.xcookie))
        fail("XGetEventData failed for PresentCompleteNotify");
      XPresentCompleteNotifyEvent *complete = event.xcookie.data;
      received_ms = monotonic_ms();
      found = complete != NULL && complete->eid == binding->event_id &&
              complete->window == binding->window &&
              complete->kind == PresentCompleteKindPixmap &&
              received_ms > binding->action_completed_ms;
      if (found) observed = *complete;
      XFreeEventData(display, &event.xcookie);
      if (found) break;
    }
    if (found) break;
    double remaining = deadline_ms - monotonic_ms();
    if (remaining <= 0.0) break;
    struct pollfd descriptor = {.fd = ConnectionNumber(display), .events = POLLIN};
    int result = poll(&descriptor, 1, (int)ceil(remaining));
    if (result < 0 && errno == EINTR) continue;
    if (result < 0) fail("poll failed waiting for PresentCompleteNotify");
    if (result == 0) break;
  }
  XPresentFreeInput(display, binding->window, binding->event_id);
  XSync(display, False);
  if (!found) fail("timed out waiting for causal PresentCompleteNotify");
  double latency_ms = received_ms - binding->input_ms;
  printf("present\t{\"schema_version\":2,\"observer\":\"native-x11-present-observer-v1\",\"observer_pid\":%ld,\"window_id\":\"%lu\",\"input_window_id\":\"%lu\",\"verified_input_window_id\":\"%lu\",\"present_window_id\":\"%lu\",\"input_target_relation\":\"same-window\",\"input_api\":\"XTEST\",\"action\":\"wheel\",\"action_token\":\"%s\",\"action_sequence\":%ld,\"action_event_count\":%ld,\"action_position\":\"terminal\",\"correlation_method\":\"observer-owned-terminal-XTEST-action-after-event-drain\",\"input_clock\":\"CLOCK_MONOTONIC\",\"completion_clock\":\"CLOCK_MONOTONIC\",\"completion_signal\":\"X11-PresentCompleteNotify\",\"physical_scanout_observed\":false,\"target_viewable_before_action\":true,\"target_width\":%d,\"target_height\":%d,\"present_extension_major\":%d,\"present_extension_minor\":%d,\"input_monotonic_ms\":%.6f,\"action_completed_monotonic_ms\":%.6f,\"present_complete_received_monotonic_ms\":%.6f,\"input_to_present_complete_ms\":%.6f,\"present_event_id\":\"%lu\",\"present_serial\":%u,\"present_ust\":%llu,\"present_msc\":%llu,\"present_kind\":%u,\"present_mode\":%u,\"injected_samples\":[]}\n",
         (long)getpid(), (unsigned long)binding->window,
         (unsigned long)binding->window, (unsigned long)binding->window,
         (unsigned long)binding->window,
         binding->token,
         binding->sequence, event_count, binding->attributes.width,
         binding->attributes.height, binding->major, binding->minor,
         binding->input_ms, binding->action_completed_ms, received_ms,
         latency_ms, (unsigned long)binding->event_id, observed.serial_number,
         (unsigned long long)observed.ust, (unsigned long long)observed.msc,
         (unsigned int)observed.kind, (unsigned int)observed.mode);
  fflush(stdout);
}

static DamageBinding begin_damage_binding(Display *display) {
  DamageBinding binding = {0};
  const char *window_text = getenv("BP_X11_DAMAGE_WINDOW");
  if (window_text == NULL || window_text[0] == '\0') return binding;
  long window_id = parse_long(window_text, "invalid BP_X11_DAMAGE_WINDOW");
  binding.token = getenv("BP_X11_DAMAGE_ACTION_TOKEN");
  const char *sequence_text = getenv("BP_X11_DAMAGE_ACTION_SEQUENCE");
  if (binding.token == NULL || binding.token[0] == '\0' ||
      sequence_text == NULL)
    fail("damage binding token or sequence is missing");
  size_t token_length = strlen(binding.token);
  if (token_length > 96) fail("damage binding token is too long");
  for (size_t index = 0; index < token_length; index += 1) {
    char value = binding.token[index];
    if (!((value >= 'a' && value <= 'z') ||
          (value >= 'A' && value <= 'Z') ||
          (value >= '0' && value <= '9') || value == '-' || value == '_' ||
          value == ':' || value == '.'))
      fail("damage binding token contains an invalid character");
  }
  binding.sequence =
      parse_long(sequence_text, "invalid damage action sequence");
  if (window_id <= 0 || binding.sequence < 0)
    fail("damage binding identity is invalid");
  binding.window = (Window)window_id;
  if (!XGetWindowAttributes(display, binding.window, &binding.attributes) ||
      binding.attributes.map_state != IsViewable)
    fail("damage binding target is not viewable");
  int error_base = 0;
  if (!XDamageQueryExtension(display, &binding.event_base, &error_base) ||
      !XDamageQueryVersion(display, &binding.major, &binding.minor))
    fail("X11 Damage extension is unavailable");
  binding.damage =
      XDamageCreate(display, binding.window, XDamageReportNonEmpty);
  if (binding.damage == None) fail("XDamageCreate failed");
  XDamageSubtract(display, binding.damage, None, None);
  XSync(display, False);
  drain_x11_events(display);
  binding.active = 1;
  return binding;
}

static void mark_damage_terminal_input(Display *display,
                                       DamageBinding *binding) {
  if (!binding->active) return;
  XDamageSubtract(display, binding->damage, None, None);
  XSync(display, False);
  drain_x11_events(display);
  binding->input_ms = monotonic_ms();
}

static void finish_damage_binding(Display *display, DamageBinding *binding,
                                  long event_count) {
  if (!binding->active) return;
  binding->action_completed_ms = monotonic_ms();
  double deadline_ms = binding->input_ms + 5000.0;
  XDamageNotifyEvent observed;
  memset(&observed, 0, sizeof(observed));
  double received_ms = 0.0;
  int found = 0;
  while (!found && monotonic_ms() < deadline_ms) {
    while (XPending(display) > 0) {
      XEvent event;
      XNextEvent(display, &event);
      if (event.type != binding->event_base + XDamageNotify) continue;
      XDamageNotifyEvent *notification = (XDamageNotifyEvent *)&event;
      received_ms = monotonic_ms();
      found = notification->damage == binding->damage &&
              notification->drawable == binding->window &&
              notification->level == XDamageReportNonEmpty &&
              notification->area.width > 0 &&
              notification->area.height > 0 &&
              received_ms > binding->action_completed_ms;
      if (found) observed = *notification;
      if (found) break;
    }
    if (found) break;
    double remaining = deadline_ms - monotonic_ms();
    if (remaining <= 0.0) break;
    struct pollfd descriptor = {
        .fd = ConnectionNumber(display), .events = POLLIN};
    int result = poll(&descriptor, 1, (int)ceil(remaining));
    if (result < 0 && errno == EINTR) continue;
    if (result < 0) fail("poll failed waiting for DamageNotify");
    if (result == 0) break;
  }
  XDamageDestroy(display, binding->damage);
  XSync(display, False);
  if (!found) fail("timed out waiting for target DamageNotify");
  double latency_ms = received_ms - binding->input_ms;
  printf("damage\t{\"schema_version\":3,\"observer\":\"native-x11-damage-observer-v1\",\"observer_pid\":%ld,\"window_id\":\"%lu\",\"input_window_id\":\"%lu\",\"verified_input_window_id\":\"%lu\",\"damage_drawable_id\":\"%lu\",\"input_target_relation\":\"same-window\",\"input_api\":\"XTEST\",\"action\":\"wheel\",\"action_token\":\"%s\",\"action_sequence\":%ld,\"action_event_count\":%ld,\"action_position\":\"terminal\",\"correlation_method\":\"observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset\",\"input_clock\":\"CLOCK_MONOTONIC\",\"completion_clock\":\"CLOCK_MONOTONIC\",\"completion_signal\":\"X11-DamageNotify\",\"observation_scope\":\"x11-server-drawable-damage-not-presentation-completion\",\"server_observed_drawable_damage\":true,\"presentation_completion_observed\":false,\"physical_scanout_observed\":false,\"target_viewable_before_action\":true,\"target_width\":%d,\"target_height\":%d,\"damage_extension_major\":%d,\"damage_extension_minor\":%d,\"damage_report_level\":\"XDamageReportNonEmpty\",\"input_monotonic_ms\":%.6f,\"action_completed_monotonic_ms\":%.6f,\"damage_notify_received_monotonic_ms\":%.6f,\"input_to_damage_notify_ms\":%.6f,\"damage_handle_id\":\"%lu\",\"damage_server_timestamp\":%lu,\"damage_area\":{\"x\":%d,\"y\":%d,\"width\":%u,\"height\":%u},\"damage_geometry\":{\"x\":%d,\"y\":%d,\"width\":%u,\"height\":%u},\"damage_more\":%s,\"injected_samples\":[]}\n",
         (long)getpid(), (unsigned long)binding->window,
         (unsigned long)binding->window, (unsigned long)binding->window,
         (unsigned long)binding->window, binding->token, binding->sequence,
         event_count, binding->attributes.width, binding->attributes.height,
         binding->major, binding->minor, binding->input_ms,
         binding->action_completed_ms, received_ms, latency_ms,
         (unsigned long)binding->damage, (unsigned long)observed.timestamp,
         observed.area.x, observed.area.y, observed.area.width,
         observed.area.height, observed.geometry.x, observed.geometry.y,
         observed.geometry.width, observed.geometry.height,
         observed.more ? "true" : "false");
  fflush(stdout);
}

static void emit_clock_sample(long index, int64_t scheduled_ns, const char *action) {
  printf("%ld\t%.9f\t%.6f\t%s\n", index, (double)scheduled_ns / 1000000.0,
         monotonic_ms(), action);
  fflush(stdout);
}

static unsigned char component_to_u8(unsigned long pixel, unsigned long mask) {
  if (mask == 0) fail("capture drawable has an empty RGB mask");
  unsigned int shift = 0;
  while (((mask >> shift) & 1UL) == 0UL) shift += 1;
  unsigned long maximum = mask >> shift;
  unsigned long value = (pixel & mask) >> shift;
  return (unsigned char)((value * 255UL + maximum / 2UL) / maximum);
}

static void require_capture_token(const char *value, const char *label) {
  if (value == NULL || value[0] == '\0' || strchr(value, '\t') != NULL
      || strchr(value, '\n') != NULL || strchr(value, '\r') != NULL)
    fail(label);
}

static void capture_presented_drawable(Display *display, Window window,
                                       int expected_width, int expected_height,
                                       const char *capture_id, const char *path) {
  require_capture_token(capture_id, "invalid capture id");
  require_capture_token(path, "invalid capture path");
  XWindowAttributes attributes;
  if (!XGetWindowAttributes(display, window, &attributes))
    fail("capture window no longer exists");
  if (attributes.map_state != IsViewable)
    fail("capture window is not viewable");
  if (attributes.width != expected_width || attributes.height != expected_height)
    fail("capture window dimensions changed");
  double started_ms = monotonic_ms();
  XImage *image = XGetImage(display, window, 0, 0, (unsigned int)expected_width,
                            (unsigned int)expected_height, AllPlanes, ZPixmap);
  if (image == NULL) fail("XGetImage failed");
  int descriptor = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
  if (descriptor < 0) {
    XDestroyImage(image);
    fail("capture artifact must not already exist and must be writable");
  }
  FILE *output = fdopen(descriptor, "wb");
  if (output == NULL) {
    close(descriptor);
    XDestroyImage(image);
    fail("fdopen failed for capture artifact");
  }
  char header[64];
  int header_bytes = snprintf(header, sizeof(header), "P6\n%d %d\n255\n",
                              expected_width, expected_height);
  if (header_bytes <= 0 || (size_t)header_bytes >= sizeof(header)
      || fwrite(header, 1U, (size_t)header_bytes, output)
           != (size_t)header_bytes)
    fail("capture header write failed");
  Sha256 artifact_hash;
  sha256_init(&artifact_hash);
  sha256_update(&artifact_hash, (const unsigned char *)header,
                (size_t)header_bytes);
  unsigned char *row = malloc((size_t)expected_width * 3U);
  if (row == NULL) fail("capture row allocation failed");
  for (int y = 0; y < expected_height; y += 1) {
    for (int x = 0; x < expected_width; x += 1) {
      unsigned long pixel = XGetPixel(image, x, y);
      row[(size_t)x * 3U] = component_to_u8(pixel, image->red_mask);
      row[(size_t)x * 3U + 1U] = component_to_u8(pixel, image->green_mask);
      row[(size_t)x * 3U + 2U] = component_to_u8(pixel, image->blue_mask);
    }
    if (fwrite(row, 3U, (size_t)expected_width, output)
        != (size_t)expected_width)
      fail("capture pixel write failed");
    sha256_update(&artifact_hash, row, (size_t)expected_width * 3U);
  }
  free(row);
  char artifact_sha256[65];
  sha256_final_hex(&artifact_hash, artifact_sha256);
  if (fclose(output) != 0) fail("capture artifact close failed");
  XDestroyImage(image);
  double ended_ms = monotonic_ms();
  printf("capture\t%s\t%.6f\t%.6f\t%lu\t%d\t%d\t%d\t%s\t%s\n",
         capture_id, started_ms, ended_ms, (unsigned long)window,
         expected_width, expected_height, attributes.depth, artifact_sha256,
         path);
  fflush(stdout);
}

static int run_capture_server(Display *display, int argc, char **argv) {
  if (argc != 5)
    fail("usage: capture-server window_id expected_width expected_height");
  long window_id = parse_long(argv[2], "invalid capture window id");
  long expected_width = parse_long(argv[3], "invalid capture width");
  long expected_height = parse_long(argv[4], "invalid capture height");
  if (window_id <= 0 || expected_width <= 0 || expected_height <= 0)
    fail("invalid capture server target");
  Window window = (Window)window_id;
  XWindowAttributes attributes;
  if (!XGetWindowAttributes(display, window, &attributes))
    fail("capture server target does not exist");
  if (attributes.map_state != IsViewable || attributes.width != expected_width
      || attributes.height != expected_height)
    fail("capture server target is not the verified viewable client drawable");
  printf("ready\t%.6f\t%lu\t%d\t%d\t%d\n", monotonic_ms(),
         (unsigned long)window, attributes.width, attributes.height,
         attributes.depth);
  fflush(stdout);
  char *line = NULL;
  size_t capacity = 0;
  while (getline(&line, &capacity, stdin) >= 0) {
    line[strcspn(line, "\r\n")] = '\0';
    if (strcmp(line, "close") == 0) break;
    char *save = NULL;
    char *operation = strtok_r(line, "\t", &save);
    char *capture_id = strtok_r(NULL, "\t", &save);
    char *path = strtok_r(NULL, "\t", &save);
    char *extra = strtok_r(NULL, "\t", &save);
    if (operation == NULL || strcmp(operation, "capture") != 0
        || capture_id == NULL || path == NULL || extra != NULL)
      fail("capture server received an invalid command");
    capture_presented_drawable(display, window, (int)expected_width,
                               (int)expected_height, capture_id, path);
  }
  free(line);
  printf("closed\t%.6f\n", monotonic_ms());
  fflush(stdout);
  return 0;
}

static int run_clock(int argc, char **argv) {
  if (argc != 4) fail("usage: clock duration_ms rate_hz");
  long duration_ms = parse_long(argv[2], "invalid clock duration");
  long rate_hz = parse_long(argv[3], "invalid clock rate");
  long intervals = duration_ms * rate_hz / 1000;
  if (duration_ms <= 0 || rate_hz <= 0
      || intervals * 1000 != duration_ms * rate_hz) fail("invalid clock schedule");
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  emit_clock_sample(0, 0, "observe");
  for (long index = 1; index <= intervals; index += 1) {
    int64_t offset_ns = (int64_t)duration_ms * 1000000LL * index / intervals;
    wait_until(add_ns(start, offset_ns));
    emit_clock_sample(index, offset_ns, "observe");
  }
  return 0;
}

static int run_self_test(int argc) {
  if (argc != 2) fail("usage: self-test");
  if (component_to_u8(0x000000UL, 0xff0000UL) != 0
      || component_to_u8(0x7f0000UL, 0xff0000UL) != 127
      || component_to_u8(0xff0000UL, 0xff0000UL) != 255
      || component_to_u8(0x07e0UL, 0x07e0UL) != 255
      || component_to_u8(0x0010UL, 0x001fUL) != 132)
    fail("RGB mask conversion self-test failed");
  Sha256 hash;
  char digest[65];
  sha256_init(&hash);
  sha256_update(&hash, (const unsigned char *)"abc", 3U);
  sha256_final_hex(&hash, digest);
  if (strcmp(digest,
             "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
      != 0)
    fail("SHA-256 self-test failed");
  printf("self-test-ok\n");
  return 0;
}

static int run_pointer(Display *display, int screen, int argc, char **argv) {
  if (argc < 7) fail("usage: pointer duration_ms count button screen_x screen_y ...");
  long duration_ms = parse_long(argv[2], "invalid duration");
  long count = parse_long(argv[3], "invalid count");
  long button = parse_long(argv[4], "invalid button");
  if (duration_ms <= 0 || count < 2 || button < 1 || button > 3
      || argc != 5 + count * 2) fail("invalid pointer schedule");
  int x = (int)parse_long(argv[5], "invalid x");
  int y = (int)parse_long(argv[6], "invalid y");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("initial motion failed");
  if (!XTestFakeButtonEvent(display, (unsigned int)button, True, CurrentTime)) fail("button press failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  emit_clock_sample(0, 0, "down");
  for (long index = 1; index < count; index += 1) {
    int64_t offset_ns = (int64_t)duration_ms * 1000000LL * index / (count - 1);
    wait_until(add_ns(start, offset_ns));
    x = (int)parse_long(argv[5 + index * 2], "invalid x");
    y = (int)parse_long(argv[6 + index * 2], "invalid y");
    if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("motion failed");
    XFlush(display);
    emit_clock_sample(index, offset_ns, "move");
  }
  if (!XTestFakeButtonEvent(display, (unsigned int)button, False, CurrentTime)) fail("button release failed");
  XFlush(display);
  return 0;
}

static int run_wheel(Display *display, int screen, int argc, char **argv) {
  if (argc != 9) fail("usage: wheel forward_ms pause_ms reverse_ms rate_hz reverse_notches screen_x screen_y");
  long forward_ms = parse_long(argv[2], "invalid forward duration");
  long pause_ms = parse_long(argv[3], "invalid pause duration");
  long reverse_ms = parse_long(argv[4], "invalid reverse duration");
  long rate_hz = parse_long(argv[5], "invalid rate");
  long reverse_notches = parse_long(argv[6], "invalid reverse notch multiplier");
  int x = (int)parse_long(argv[7], "invalid x");
  int y = (int)parse_long(argv[8], "invalid y");
  long forward_count = forward_ms * rate_hz / 1000;
  long reverse_count = reverse_ms * rate_hz / 1000;
  if (forward_ms <= 0 || pause_ms < 0 || reverse_ms <= 0 || rate_hz <= 0
      || reverse_notches < 1 || forward_count * 1000 != forward_ms * rate_hz
      || reverse_count * 1000 != reverse_ms * rate_hz) fail("invalid wheel schedule");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("wheel target motion failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  for (long index = 0; index < forward_count; index += 1) {
    wait_until(add_ns(start, (int64_t)forward_ms * 1000000LL * (index + 1) / forward_count));
    if (!XTestFakeButtonEvent(display, 5, True, CurrentTime)
        || !XTestFakeButtonEvent(display, 5, False, CurrentTime)) fail("forward wheel event failed");
    XFlush(display);
  }
  int64_t reverse_start_ns = (int64_t)(forward_ms + pause_ms) * 1000000LL;
  for (long index = 0; index < reverse_count; index += 1) {
    wait_until(add_ns(start, reverse_start_ns
      + (int64_t)reverse_ms * 1000000LL * (index + 1) / reverse_count));
    for (long notch = 0; notch < reverse_notches; notch += 1) {
      if (!XTestFakeButtonEvent(display, 4, True, CurrentTime)
          || !XTestFakeButtonEvent(display, 4, False, CurrentTime)) fail("reverse wheel event failed");
    }
    XFlush(display);
  }
  return 0;
}

static int run_wheel_calibration(Display *display, int screen, int argc, char **argv) {
  if (argc != 4) fail("usage: wheel-calibration screen_x screen_y");
  int x = (int)parse_long(argv[2], "invalid x");
  int y = (int)parse_long(argv[3], "invalid y");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime))
    fail("wheel calibration target motion failed");
  if (!XTestFakeButtonEvent(display, 5, True, CurrentTime)
      || !XTestFakeButtonEvent(display, 5, False, CurrentTime))
    fail("wheel calibration event failed");
  XFlush(display);
  emit_clock_sample(0, 0, "calibration-forward-single-notch");
  return 0;
}

static int run_dynamic_wheel(Display *display, int screen, int argc, char **argv) {
  if (argc != 9) fail("usage: dynamic-wheel forward_ms pause_ms reverse_ms rate_hz reverse_notches screen_x screen_y");
  long forward_ms = parse_long(argv[2], "invalid forward duration");
  long pause_ms = parse_long(argv[3], "invalid pause duration");
  long reverse_ms = parse_long(argv[4], "invalid reverse duration");
  long rate_hz = parse_long(argv[5], "invalid rate");
  long reverse_notches = parse_long(argv[6], "invalid reverse notch multiplier");
  int x = (int)parse_long(argv[7], "invalid x");
  int y = (int)parse_long(argv[8], "invalid y");
  long duration_ms = forward_ms + pause_ms + reverse_ms;
  long intervals = duration_ms * rate_hz / 1000;
  if (forward_ms <= 0 || pause_ms < 0 || reverse_ms <= 0 || rate_hz <= 0
      || reverse_notches < 1 || intervals * 1000 != duration_ms * rate_hz)
    fail("invalid dynamic wheel schedule");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("wheel target motion failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  emit_clock_sample(0, 0, "start");
  for (long index = 1; index <= intervals; index += 1) {
    int64_t offset_ns = (int64_t)duration_ms * 1000000LL * index / intervals;
    wait_until(add_ns(start, offset_ns));
    int64_t offset_ms_times_rate = (int64_t)index * 1000LL;
    const char *action = "pause";
    if (offset_ms_times_rate <= (int64_t)forward_ms * rate_hz) {
      if (!XTestFakeButtonEvent(display, 5, True, CurrentTime)
          || !XTestFakeButtonEvent(display, 5, False, CurrentTime)) fail("forward wheel event failed");
      action = "forward";
    } else if (offset_ms_times_rate > (int64_t)(forward_ms + pause_ms) * rate_hz) {
      for (long notch = 0; notch < reverse_notches; notch += 1) {
        if (!XTestFakeButtonEvent(display, 4, True, CurrentTime)
            || !XTestFakeButtonEvent(display, 4, False, CurrentTime)) fail("reverse wheel event failed");
      }
      action = "reverse";
    }
    XFlush(display);
    emit_clock_sample(index, offset_ns, action);
  }
  return 0;
}

static int run_dynamic_wheel_distance(Display *display, int screen, int argc, char **argv) {
  if (argc != 10) fail("usage: dynamic-wheel-distance forward_ms pause_ms reverse_ms rate_hz forward_events reverse_events screen_x screen_y");
  long forward_ms = parse_long(argv[2], "invalid forward duration");
  long pause_ms = parse_long(argv[3], "invalid pause duration");
  long reverse_ms = parse_long(argv[4], "invalid reverse duration");
  long rate_hz = parse_long(argv[5], "invalid rate");
  long forward_events = parse_long(argv[6], "invalid forward event count");
  long reverse_events = parse_long(argv[7], "invalid reverse event count");
  int x = (int)parse_long(argv[8], "invalid x");
  int y = (int)parse_long(argv[9], "invalid y");
  long duration_ms = forward_ms + pause_ms + reverse_ms;
  long intervals = duration_ms * rate_hz / 1000;
  long forward_intervals = forward_ms * rate_hz / 1000;
  long pause_intervals = pause_ms * rate_hz / 1000;
  long reverse_intervals = reverse_ms * rate_hz / 1000;
  if (forward_ms <= 0 || pause_ms < 0 || reverse_ms <= 0 || rate_hz <= 0
      || forward_events < 1 || reverse_events < 1
      || intervals * 1000 != duration_ms * rate_hz
      || forward_intervals * 1000 != forward_ms * rate_hz
      || pause_intervals * 1000 != pause_ms * rate_hz
      || reverse_intervals * 1000 != reverse_ms * rate_hz)
    fail("invalid distance-bounded dynamic wheel schedule");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("wheel target motion failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  emit_clock_sample(0, 0, "start");
  long emitted_forward = 0;
  long emitted_reverse = 0;
  for (long index = 1; index <= intervals; index += 1) {
    int64_t offset_ns = (int64_t)duration_ms * 1000000LL * index / intervals;
    wait_until(add_ns(start, offset_ns));
    const char *action = "pause";
    if (index <= forward_intervals) {
      long expected = index * forward_events / forward_intervals;
      while (emitted_forward < expected) {
        if (!XTestFakeButtonEvent(display, 5, True, CurrentTime)
            || !XTestFakeButtonEvent(display, 5, False, CurrentTime)) fail("forward wheel event failed");
        emitted_forward += 1;
      }
      action = "forward";
    } else if (index > forward_intervals + pause_intervals) {
      long reverse_index = index - forward_intervals - pause_intervals;
      long expected = reverse_index * reverse_events / reverse_intervals;
      while (emitted_reverse < expected) {
        if (!XTestFakeButtonEvent(display, 4, True, CurrentTime)
            || !XTestFakeButtonEvent(display, 4, False, CurrentTime)) fail("reverse wheel event failed");
        emitted_reverse += 1;
      }
      action = "reverse";
    }
    XFlush(display);
    emit_clock_sample(index, offset_ns, action);
  }
  if (emitted_forward != forward_events || emitted_reverse != reverse_events)
    fail("distance-bounded dynamic wheel event count mismatch");
  return 0;
}

static int run_dynamic_wheel_held_distance(Display *display, int screen, int argc, char **argv) {
  if (argc != 13) fail("usage: dynamic-wheel-held-distance forward_ms pause_ms reverse_ms rate_hz forward_events reverse_events page15_events page29_events hold_intervals screen_x screen_y");
  long forward_ms = parse_long(argv[2], "invalid forward duration");
  long pause_ms = parse_long(argv[3], "invalid pause duration");
  long reverse_ms = parse_long(argv[4], "invalid reverse duration");
  long rate_hz = parse_long(argv[5], "invalid rate");
  long forward_events = parse_long(argv[6], "invalid forward event count");
  long reverse_events = parse_long(argv[7], "invalid reverse event count");
  long checkpoint_events[2] = {
    parse_long(argv[8], "invalid page 15 checkpoint"),
    parse_long(argv[9], "invalid page 29 checkpoint"),
  };
  long hold_intervals = parse_long(argv[10], "invalid hold interval count");
  int x = (int)parse_long(argv[11], "invalid x");
  int y = (int)parse_long(argv[12], "invalid y");
  long duration_ms = forward_ms + pause_ms + reverse_ms;
  long intervals = duration_ms * rate_hz / 1000;
  long forward_intervals = forward_ms * rate_hz / 1000;
  long pause_intervals = pause_ms * rate_hz / 1000;
  long reverse_intervals = reverse_ms * rate_hz / 1000;
  long checkpoint_hold_samples = hold_intervals + 1;
  PresentBinding present_binding = begin_present_binding(display);
  DamageBinding damage_binding = begin_damage_binding(display);
  long forward_motion_intervals =
    forward_intervals - hold_intervals - 2 * checkpoint_hold_samples;
  if (forward_ms <= 0 || pause_ms < 0 || reverse_ms <= 0 || rate_hz <= 0
      || forward_events < 1 || reverse_events < 1 || hold_intervals < 1
      || checkpoint_events[0] < 1
      || checkpoint_events[0] >= checkpoint_events[1]
      || checkpoint_events[1] >= forward_events
      || forward_motion_intervals < forward_events
      || intervals * 1000 != duration_ms * rate_hz
      || forward_intervals * 1000 != forward_ms * rate_hz
      || pause_intervals * 1000 != pause_ms * rate_hz
      || reverse_intervals * 1000 != reverse_ms * rate_hz)
    fail("invalid held distance-bounded dynamic wheel schedule");
  if (!XTestFakeMotionEvent(display, screen, x, y, CurrentTime)) fail("wheel target motion failed");
  XFlush(display);
  struct timespec start;
  if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("clock_gettime failed");
  emit_clock_sample(0, 0, "hold-page-1");
  long emitted_forward = 0;
  long emitted_reverse = 0;
  long forward_motion_index = 0;
  long checkpoint_index = 0;
  long checkpoint_hold_remaining = 0;
  for (long index = 1; index <= intervals; index += 1) {
    int64_t offset_ns = (int64_t)duration_ms * 1000000LL * index / intervals;
    wait_until(add_ns(start, offset_ns));
    const char *action = "pause";
    if (index <= forward_intervals) {
      if (index <= hold_intervals) {
        action = "hold-page-1";
      } else if (checkpoint_hold_remaining > 0) {
        action = checkpoint_index == 1 ? "hold-page-15" : "hold-page-29";
        checkpoint_hold_remaining -= 1;
      } else {
        forward_motion_index += 1;
        long expected = forward_motion_index * forward_events / forward_motion_intervals;
        if (checkpoint_index < 2 && expected >= checkpoint_events[checkpoint_index]) {
          expected = checkpoint_events[checkpoint_index];
        }
        while (emitted_forward < expected) {
          if (!XTestFakeButtonEvent(display, 5, True, CurrentTime)
              || !XTestFakeButtonEvent(display, 5, False, CurrentTime)) fail("forward wheel event failed");
          emitted_forward += 1;
        }
        action = "forward";
        if (checkpoint_index < 2
            && emitted_forward == checkpoint_events[checkpoint_index]) {
          checkpoint_index += 1;
          checkpoint_hold_remaining = checkpoint_hold_samples;
        }
      }
    } else if (index > forward_intervals + pause_intervals) {
      long reverse_index = index - forward_intervals - pause_intervals;
      long expected = reverse_index * reverse_events / reverse_intervals;
      while (emitted_reverse < expected) {
        if (emitted_reverse + 1 == reverse_events)
          mark_present_terminal_input(display, &present_binding);
        if (emitted_reverse + 1 == reverse_events)
          mark_damage_terminal_input(display, &damage_binding);
        if (!XTestFakeButtonEvent(display, 4, True, CurrentTime)
            || !XTestFakeButtonEvent(display, 4, False, CurrentTime)) fail("reverse wheel event failed");
        emitted_reverse += 1;
      }
      action = "reverse";
    }
    XFlush(display);
    emit_clock_sample(index, offset_ns, action);
  }
  if (emitted_forward != forward_events || emitted_reverse != reverse_events
      || checkpoint_index != 2 || checkpoint_hold_remaining != 0)
    fail("held distance-bounded dynamic wheel event count mismatch");
  finish_present_binding(display, &present_binding,
                         forward_events + reverse_events);
  finish_damage_binding(display, &damage_binding,
                        forward_events + reverse_events);
  return 0;
}

int main(int argc, char **argv) {
  if (argc < 2) fail("missing replay mode");
  if (strcmp(argv[1], "clock") == 0) return run_clock(argc, argv);
  if (strcmp(argv[1], "self-test") == 0) return run_self_test(argc);
  Display *display = XOpenDisplay(NULL);
  if (display == NULL) fail("cannot open DISPLAY");
  int screen = DefaultScreen(display);
  int result;
  if (strcmp(argv[1], "pointer") == 0) result = run_pointer(display, screen, argc, argv);
  else if (strcmp(argv[1], "capture-server") == 0) result = run_capture_server(display, argc, argv);
  else if (strcmp(argv[1], "wheel-calibration") == 0) result = run_wheel_calibration(display, screen, argc, argv);
  else if (strcmp(argv[1], "wheel") == 0) result = run_wheel(display, screen, argc, argv);
  else if (strcmp(argv[1], "dynamic-wheel") == 0) result = run_dynamic_wheel(display, screen, argc, argv);
  else if (strcmp(argv[1], "dynamic-wheel-distance") == 0) result = run_dynamic_wheel_distance(display, screen, argc, argv);
  else if (strcmp(argv[1], "dynamic-wheel-held-distance") == 0) result = run_dynamic_wheel_held_distance(display, screen, argc, argv);
  else fail("unsupported replay mode");
  XCloseDisplay(display);
  return result;
}
