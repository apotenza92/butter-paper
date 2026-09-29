#include <errno.h>
#include <inttypes.h>
#include <libproc.h>
#include <mach/mach_time.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>
#include <unistd.h>

#define MAX_PROCESSES 2048

typedef struct {
  pid_t pid;
  pid_t ppid;
  struct rusage_info_v4 usage;
} process_sample;

static uint64_t abstime_to_ns(uint64_t value) {
  mach_timebase_info_data_t timebase;
  if (mach_timebase_info(&timebase) != KERN_SUCCESS || timebase.denom == 0) {
    return 0;
  }
  return (uint64_t)(((__uint128_t)value * timebase.numer) / timebase.denom);
}

static uint64_t monotonic_ns(void) {
  struct timespec value;
  if (clock_gettime(CLOCK_MONOTONIC, &value) != 0) {
    return 0;
  }
  return (uint64_t)value.tv_sec * 1000000000ULL + (uint64_t)value.tv_nsec;
}

static int contains_pid(const pid_t *pids, size_t count, pid_t pid) {
  for (size_t index = 0; index < count; index += 1) {
    if (pids[index] == pid) {
      return 1;
    }
  }
  return 0;
}

static int read_process(pid_t pid, process_sample *sample) {
  struct proc_bsdinfo info;
  memset(&info, 0, sizeof(info));
  memset(sample, 0, sizeof(*sample));
  if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) !=
      (int)sizeof(info)) {
    return -1;
  }
  if (proc_pid_rusage(pid, RUSAGE_INFO_V4, (rusage_info_t *)&sample->usage) !=
      0) {
    return -1;
  }
  sample->pid = pid;
  sample->ppid = (pid_t)info.pbi_ppid;
  return 0;
}

static int collect_tree(pid_t root_pid, process_sample *samples,
                        size_t *sample_count) {
  pid_t pending[MAX_PROCESSES];
  size_t pending_count = 1;
  size_t cursor = 0;
  *sample_count = 0;
  pending[0] = root_pid;

  while (cursor < pending_count) {
    pid_t pid = pending[cursor++];
    process_sample sample;
    if (read_process(pid, &sample) != 0) {
      if (pid == root_pid && *sample_count == 0) {
        return 1;
      }
      continue;
    }
    if (*sample_count >= MAX_PROCESSES) {
      errno = EOVERFLOW;
      return -1;
    }
    samples[(*sample_count)++] = sample;

    pid_t children[MAX_PROCESSES];
    int child_count_result =
        proc_listchildpids(pid, children, sizeof(children));
    if (child_count_result < 0) {
      return -1;
    }
    size_t child_count = (size_t)child_count_result;
    for (size_t index = 0; index < child_count; index += 1) {
      pid_t child_pid = children[index];
      if (child_pid <= 0 || contains_pid(pending, pending_count, child_pid)) {
        continue;
      }
      if (pending_count >= MAX_PROCESSES) {
        errno = EOVERFLOW;
        return -1;
      }
      pending[pending_count++] = child_pid;
    }
  }
  return 0;
}

static void emit_sample(const process_sample *samples, size_t count) {
  printf("{\"type\":\"sample\",\"monotonic_ns\":%" PRIu64 ",\"processes\":[",
         monotonic_ns());
  for (size_t index = 0; index < count; index += 1) {
    const process_sample *sample = &samples[index];
    if (index > 0) {
      putchar(',');
    }
    printf("{\"pid\":%d,\"ppid\":%d,\"start_abstime\":%" PRIu64
           ",\"exit_abstime\":%" PRIu64 ",\"user_ns\":%" PRIu64
           ",\"system_ns\":%" PRIu64 ",\"child_user_ns\":%" PRIu64
           ",\"child_system_ns\":%" PRIu64 ",\"phys_footprint_bytes\":%" PRIu64
           ",\"lifetime_max_phys_footprint_bytes\":%" PRIu64 "}",
           sample->pid, sample->ppid, sample->usage.ri_proc_start_abstime,
           sample->usage.ri_proc_exit_abstime,
           abstime_to_ns(sample->usage.ri_user_time),
           abstime_to_ns(sample->usage.ri_system_time),
           abstime_to_ns(sample->usage.ri_child_user_time),
           abstime_to_ns(sample->usage.ri_child_system_time),
           sample->usage.ri_phys_footprint,
           sample->usage.ri_lifetime_max_phys_footprint);
  }
  puts("]}");
  fflush(stdout);
}

static void sleep_ms(unsigned interval_ms) {
  struct timespec requested = {
      .tv_sec = interval_ms / 1000,
      .tv_nsec = (long)(interval_ms % 1000) * 1000000L,
  };
  while (nanosleep(&requested, &requested) != 0 && errno == EINTR) {
  }
}

static int emit_identity(pid_t pid) {
  process_sample sample;
  if (read_process(pid, &sample) != 0) {
    fprintf(stderr, "could not read process identity for %d: %s\n", pid,
            strerror(errno));
    return 69;
  }
  printf("{\"type\":\"identity\",\"pid\":%d,\"ppid\":%d,"
         "\"start_abstime\":%" PRIu64 ",\"user_ns\":%" PRIu64
         ",\"system_ns\":%" PRIu64 ",\"phys_footprint_bytes\":%" PRIu64
         ",\"lifetime_max_phys_footprint_bytes\":%" PRIu64 "}\n",
         sample.pid, sample.ppid, sample.usage.ri_proc_start_abstime,
         abstime_to_ns(sample.usage.ri_user_time),
         abstime_to_ns(sample.usage.ri_system_time),
         sample.usage.ri_phys_footprint,
         sample.usage.ri_lifetime_max_phys_footprint);
  return 0;
}

static int valid_token(const char *token) {
  if (*token == '\0') {
    return 0;
  }
  for (const unsigned char *cursor = (const unsigned char *)token; *cursor;
       cursor += 1) {
    if (!((*cursor >= 'a' && *cursor <= 'z') ||
          (*cursor >= 'A' && *cursor <= 'Z') ||
          (*cursor >= '0' && *cursor <= '9') || *cursor == '-' ||
          *cursor == '_')) {
      return 0;
    }
  }
  return 1;
}

static int run_registered_child(const char *token) {
  if (!valid_token(token)) {
    fprintf(stderr, "invalid registration token\n");
    return 64;
  }
  process_sample started;
  if (read_process(getpid(), &started) != 0 ||
      started.usage.ri_proc_start_abstime == 0) {
    fprintf(stderr, "could not capture registered-child identity\n");
    return 69;
  }
  printf("{\"type\":\"child-register\",\"token\":\"%s\","
         "\"pid\":%d,\"ppid\":%d,\"start_abstime\":%" PRIu64 "}\n",
         token, started.pid, started.ppid, started.usage.ri_proc_start_abstime);
  fflush(stdout);

  const size_t allocation_size = 24U * 1024U * 1024U;
  unsigned char *allocation = malloc(allocation_size);
  if (allocation == NULL) {
    fprintf(stderr, "registered-child allocation failed\n");
    return 70;
  }
  memset(allocation, 2, allocation_size);
  volatile uint64_t checksum = 0;
  uint64_t deadline = monotonic_ns() + 35000000ULL;
  while (monotonic_ns() < deadline) {
    checksum += allocation[checksum % allocation_size];
  }

  process_sample final;
  if (read_process(getpid(), &final) != 0 ||
      final.usage.ri_proc_start_abstime !=
          started.usage.ri_proc_start_abstime) {
    free(allocation);
    fprintf(stderr, "registered-child identity changed before final receipt\n");
    return 70;
  }
  printf("{\"type\":\"child-final\",\"token\":\"%s\","
         "\"pid\":%d,\"start_abstime\":%" PRIu64 ",\"user_ns\":%" PRIu64
         ",\"system_ns\":%" PRIu64
         ",\"lifetime_max_phys_footprint_bytes\":%" PRIu64
         ",\"checksum\":%" PRIu64 "}\n",
         token, final.pid, final.usage.ri_proc_start_abstime,
         abstime_to_ns(final.usage.ri_user_time),
         abstime_to_ns(final.usage.ri_system_time),
         final.usage.ri_lifetime_max_phys_footprint, checksum);
  fflush(stdout);
  free(allocation);
  return 0;
}

int main(int argc, char **argv) {
  if (argc == 3 && strcmp(argv[1], "--registered-child") == 0) {
    return run_registered_child(argv[2]);
  }
  if (argc == 3 && strcmp(argv[1], "--identity") == 0) {
    char *identity_end = NULL;
    long identity_pid = strtol(argv[2], &identity_end, 10);
    if (*argv[2] == '\0' || *identity_end != '\0' || identity_pid <= 0) {
      fprintf(stderr, "invalid identity pid\n");
      return 64;
    }
    return emit_identity((pid_t)identity_pid);
  }
  if (argc != 4) {
    fprintf(stderr,
            "usage: %s ROOT_PID INTERVAL_MS TIMEOUT_MS | --identity PID | "
            "--registered-child TOKEN\n",
            argv[0]);
    return 64;
  }
  char *end = NULL;
  long parsed_pid = strtol(argv[1], &end, 10);
  if (*argv[1] == '\0' || *end != '\0' || parsed_pid <= 0) {
    fprintf(stderr, "invalid root pid\n");
    return 64;
  }
  long interval_ms = strtol(argv[2], &end, 10);
  if (*argv[2] == '\0' || *end != '\0' || interval_ms < 1 ||
      interval_ms > 100) {
    fprintf(stderr, "invalid interval\n");
    return 64;
  }
  long timeout_ms = strtol(argv[3], &end, 10);
  if (*argv[3] == '\0' || *end != '\0' || timeout_ms < interval_ms ||
      timeout_ms > 180000) {
    fprintf(stderr, "invalid timeout\n");
    return 64;
  }

  uint64_t started = monotonic_ns();
  process_sample samples[MAX_PROCESSES];
  for (;;) {
    size_t count = 0;
    int result = collect_tree((pid_t)parsed_pid, samples, &count);
    if (result == 1) {
      printf("{\"type\":\"root-exited\",\"monotonic_ns\":%" PRIu64 "}\n",
             monotonic_ns());
      fflush(stdout);
      return 0;
    }
    if (result != 0) {
      fprintf(stderr, "libproc collection failed: %s\n", strerror(errno));
      return 70;
    }
    emit_sample(samples, count);
    uint64_t elapsed_ms = (monotonic_ns() - started) / 1000000ULL;
    if (elapsed_ms >= (uint64_t)timeout_ms) {
      fprintf(stderr, "monitor timed out while root remained live\n");
      return 124;
    }
    sleep_ms((unsigned)interval_ms);
  }
}
