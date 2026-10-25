/* SPDX-License-Identifier: MIT */
#ifndef CHILPA_PIPEWIRE_DSD_H
#define CHILPA_PIPEWIRE_DSD_H

#include <pipewire/pipewire.h>
#include <spa/param/audio/dsd-utils.h>
#include <spa/param/buffers.h>
#include <spa/utils/result.h>
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#define DSD_FRAME_BYTES 8u
#define DSD_MAX_BUFFERS 8u
#define DSD_MAX_BUFFER_BYTES 65536u
#define DSD_MAX_LINKS 8u
#define DSD_ERROR_BYTES 256u
#define DSD_OPEN_NS 800000000ll

struct dsd_caps {
    uint64_t serial;
    uint32_t be_rates;
    uint32_t le_rates;
};

struct dsd_session;
struct dsd_link {
    struct dsd_session *session;
    struct pw_link *proxy;
    struct spa_hook listener;
    uint32_t id;
    bool valid;
    bool format_valid;
};

/* All callbacks run on the owned thread loop, including process (no RT_PROCESS).
 * Calls from the producer hold that same lock. No callback waits, allocates a
 * payload buffer, logs, or touches the filesystem. Each live sink owns at most
 * a half-second ring, eight 64 KiB mapped buffers, eight links and one loop.
 * Query sessions have no stream/ring and are destroyed after the roundtrip. */
struct dsd_session {
    struct pw_thread_loop *loop;
    struct pw_context *context;
    struct pw_core *core;
    struct pw_registry *registry;
    struct pw_node *target;
    struct pw_stream *stream;
    struct spa_hook core_listener, registry_listener, node_listener, stream_listener;
    struct dsd_link links[DSD_MAX_LINKS];
    const char *name;
    struct dsd_caps caps;
    uint64_t expected_serial;
    uint32_t core_cookie, expected_cookie;
    uint32_t target_id;
    uint32_t rate;
    int32_t interleave;
    int sync_seq, done_seq, error;
    char message[DSD_ERROR_BYTES];
    enum pw_stream_state state;
    bool started_loop, physical, closing, opened, format_valid;
    bool active, paused, draining, drain_requested, drained;
    bool xrun;
    uint32_t buffer_count, param_count;
    uint8_t *ring;
    size_t capacity;
    uint64_t read_at, write_at;
    uint64_t submitted;
    uint64_t last_delay;
};

bool dsd_choice_contains(uint32_t type, const int32_t *values, uint32_t count, int32_t wanted);
bool dsd_format_matches(const struct spa_pod *pod, uint32_t rate, int32_t interleave);
void dsd_collect_format(const struct spa_pod *pod, struct dsd_caps *caps);
size_t dsd_ring_write(struct dsd_session *s, const uint8_t *bytes, size_t frames);
size_t dsd_ring_read(struct dsd_session *s, uint8_t *bytes, size_t frames);
void dsd_fail(struct dsd_session *s, int error, const char *message);
bool dsd_route_ready(struct dsd_session *s);
int dsd_roundtrip(struct dsd_session *s, const struct timespec *deadline);
struct dsd_session *dsd_session_new(const char *name, uint64_t serial, uint32_t cookie, struct timespec *deadline);
int dsd_session_start_stream(struct dsd_session *s, const struct timespec *deadline);

int chilpa_pw_dsd_query(const char *name, struct dsd_caps *caps, char *error, size_t size);
struct dsd_session *chilpa_pw_dsd_open(const char *name, uint64_t serial,
        uint32_t cookie, uint32_t rate, int32_t interleave, char *error, size_t size);
int64_t chilpa_pw_dsd_write(struct dsd_session *s, const uint8_t *bytes, size_t size);
int chilpa_pw_dsd_pause(struct dsd_session *s, bool paused);
int chilpa_pw_dsd_flush(struct dsd_session *s);
int chilpa_pw_dsd_drain(struct dsd_session *s);
uint64_t chilpa_pw_dsd_held(struct dsd_session *s);
void chilpa_pw_dsd_error(struct dsd_session *s, char *error, size_t size);
uint64_t chilpa_pw_dsd_serial(struct dsd_session *s);
uint32_t chilpa_pw_dsd_cookie(struct dsd_session *s);
void chilpa_pw_dsd_close(struct dsd_session *s);

#endif
