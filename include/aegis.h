#ifndef AEGIS_H
#define AEGIS_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

double aegis_filter_move_velocity(double demand, double dt);
double aegis_filter_rotate_velocity(double angular_demand, double dt);
uint32_t aegis_get_trust_score(void);
int aegis_reset_state(const uint8_t *token_ptr, size_t token_len);

#ifdef __cplusplus
}
#endif

#endif /* AEGIS_H */
