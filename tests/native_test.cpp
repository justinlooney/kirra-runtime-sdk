// tests/native_test.cpp
// Native C++ integration test for the Kirra FFI layer.

#include <cassert>
#include <cstdio>
#include <cstring>
#include "../include/kirra.h"

int main() {
    printf("=== Kirra Native FFI Integration Test ===\n\n");

    // Velocity within envelope — expect passthrough
    double safe_result = kirra_filter_move_velocity(1.0, 0.1);
    printf("Safe velocity 1.0 -> %.4f\n", safe_result);
    assert(safe_result >= 0.9 && safe_result <= 1.1);

    // Velocity beyond envelope — expect clamping to max_linear_velocity (2.0)
    double clamped_result = kirra_filter_move_velocity(10.0, 0.1);
    printf("Over-limit velocity 10.0 -> %.4f (clamped)\n", clamped_result);
    assert(clamped_result <= 2.0);

    // Angular velocity within limit (1.5) — expect passthrough
    double safe_angular = kirra_filter_rotate_velocity(1.0, 0.1);
    printf("Safe angular 1.0 -> %.4f\n", safe_angular);
    assert(safe_angular >= 0.9 && safe_angular <= 1.1);

    // Angular velocity over limit — expect clamping
    double clamped_angular = kirra_filter_rotate_velocity(5.0, 0.1);
    printf("Over-limit angular 5.0 -> %.4f (clamped)\n", clamped_angular);
    assert(clamped_angular <= 1.5);

    // Trust score should be available
    uint32_t score = kirra_get_trust_score();
    printf("Trust score: %u\n", score);
    assert(score <= 100);

    // Reset without env key — must return 0 (fail-closed)
    const uint8_t token[] = "test_token";
    int reset_result = kirra_reset_state(token, strlen((const char*)token));
    printf("Reset without env key: %d (expected 0)\n", reset_result);
    assert(reset_result == 0);

    printf("\nAll assertions passed.\n");
    return 0;
}
