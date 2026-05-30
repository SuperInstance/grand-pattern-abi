#ifndef GRAND_PATTERN_H
#define GRAND_PATTERN_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Types ──────────────────────────────────────────── */

typedef struct {
    double dims[16];
} CVibe;

typedef struct {
    uint8_t id[36];          /* UUID string */
    uint8_t name[64];
    CVibe vibe;
    size_t perception_count;
    size_t prediction_count;
    uint64_t tick_count;
    double surprise;
} CRoom;

typedef struct {
    uint8_t source[36];
    CVibe vibe_snapshot;
    double surprise_avg;
    uint64_t tick;
    uint32_t ttl;
    uint8_t level;           /* 0=neighbor, 1=zone, 2=fleet */
} CMurmur;

typedef struct {
    size_t room_count;
    size_t edge_count;
    CVibe fleet_vibe;
    double fleet_surprise;
    uint64_t tick;
} CGraph;

/* ── Vibe ───────────────────────────────────────────── */

CVibe      vibe_new(void);
CVibe      vibe_blend(CVibe a, CVibe b, double ratio);
double     vibe_distance(CVibe a, CVibe b);
double     vibe_energy(CVibe v);

/* ── Room ───────────────────────────────────────────── */

CRoom      room_create(const char *name);
void       room_perceive(CRoom *room, const double *data, size_t dim);
void       room_predict(const CRoom *room, double *out, size_t dim);
void       room_tick(CRoom *room);
double     room_surprise(const CRoom *room);

/* ── Graph ──────────────────────────────────────────── */

CGraph     graph_create(double bpm);
uint32_t   graph_add_room(CGraph *graph, const char *name);
void       graph_add_edge(CGraph *graph, uint32_t from, uint32_t to);
void       graph_tick(CGraph *graph);
void       graph_gossip(CGraph *graph);
CVibe      graph_fleet_vibe(const CGraph *graph);
uint32_t   graph_detect_anomaly(const CGraph *graph, double threshold);

/* ── Tick ───────────────────────────────────────────── */

double     tick_interval_bpm(double bpm);
double     swing_offset(double bpm, double swing, bool is_offbeat);

/* ── Murmur ─────────────────────────────────────────── */

CMurmur    murmur_create(const char *source, CVibe vibe, double surprise, uint64_t tick);
void       murmur_decay(CMurmur *murmur);
bool       murmur_is_expired(const CMurmur *murmur);

#ifdef __cplusplus
}
#endif

#endif /* GRAND_PATTERN_H */
