/**
 * Platform layer for the VL53L5CX ULD, CoworkOS port.
 *
 * ST ships this header as a template to be filled in per platform — see
 * "Structure VL53L5CX_Platform needs to be filled by the customer" in the
 * original. This is that fill-in, and it is the only vendor file we modify;
 * everything under ../inc and ../src is ST's drop, unchanged, so the ULD can
 * be updated by replacing those directories.
 *
 * The seven platform functions are implemented in Rust rather than C — see
 * src/vl53l5cx/platform.rs. Only three of them touch hardware, which keeps the
 * unsafe boundary narrow enough to audit in one sitting. There is deliberately
 * no platform.c in this tree.
 *
 * Derived from the ULD 2.0.1 platform template.
 * Copyright (c) 2021 STMicroelectronics. Licensed under the terms in the
 * LICENSE file shipped with the ULD.
 */

#ifndef _PLATFORM_H_
#define _PLATFORM_H_
#pragma once

#include <stdint.h>

/**
 * @brief Per-sensor platform context.
 *
 * `address` is the ULD's own convention: an **8-bit** I2C address, so the
 * default VL53L5CX_DEFAULT_I2C_ADDRESS is 0x52 rather than 0x29. ST's HAL
 * takes 8-bit addresses too, which is why their reference platform.c passes
 * this field straight to HAL_I2C_*. Embassy takes 7-bit addresses, so the
 * Rust side shifts right by one. Getting this wrong produces a sensor that
 * never acknowledges, with nothing in the driver to suggest why.
 *
 * `ctx` is an opaque pointer back to the Rust-side bus handle. It exists so
 * the platform functions can find their I2C peripheral without a global, and
 * so a second sensor on the same bus stays possible without redesign.
 */
typedef struct
{
	uint16_t	address;
	void		*ctx;

} VL53L5CX_Platform;

/*
 * @brief The macro below is used to define the number of target per zone sent
 * through I2C. This value can be changed by user, in order to tune I2C
 * transaction, and also the total memory size (a lower number of target per
 * zone means a lower RAM). The value must be between 1 and 4.
 *
 * Held at 1: occupancy needs one distance per zone, and raising it costs both
 * RAM and I2C time on every frame.
 */

#define 	VL53L5CX_NB_TARGET_PER_ZONE		1U

/*
 * @brief The macro below can be used to avoid data conversion into the driver.
 * By default there is a conversion between firmware and user data. Using this macro
 * allows to use the firmware format instead of user format. The firmware format allows
 * an increased precision.
 */

// #define 	VL53L5CX_USE_RAW_FORMAT

/*
 * @brief All macro below are used to configure the sensor output. User can
 * define some macros if he wants to disable selected output, in order to reduce
 * I2C access.
 *
 * All left enabled for now. Phase 2 wants distance and reflectance per zone;
 * these are the lever to pull if the per-frame I2C transfer needs trimming.
 */

// #define VL53L5CX_DISABLE_AMBIENT_PER_SPAD
// #define VL53L5CX_DISABLE_NB_SPADS_ENABLED
// #define VL53L5CX_DISABLE_NB_TARGET_DETECTED
// #define VL53L5CX_DISABLE_SIGNAL_PER_SPAD
// #define VL53L5CX_DISABLE_RANGE_SIGMA_MM
// #define VL53L5CX_DISABLE_DISTANCE_MM
// #define VL53L5CX_DISABLE_REFLECTANCE_PERCENT
// #define VL53L5CX_DISABLE_TARGET_STATUS
// #define VL53L5CX_DISABLE_MOTION_INDICATOR

/**
 * @brief Mandatory function used to read one single byte.
 */
uint8_t VL53L5CX_RdByte(
		VL53L5CX_Platform *p_platform,
		uint16_t RegisterAdress,
		uint8_t *p_value);

/**
 * @brief Mandatory function used to write one single byte.
 */
uint8_t VL53L5CX_WrByte(
		VL53L5CX_Platform *p_platform,
		uint16_t RegisterAdress,
		uint8_t value);

/**
 * @brief Mandatory function used to read multiples bytes.
 */
uint8_t VL53L5CX_RdMulti(
		VL53L5CX_Platform *p_platform,
		uint16_t RegisterAdress,
		uint8_t *p_values,
		uint32_t size);

/**
 * @brief Mandatory function used to write multiples bytes.
 */
uint8_t VL53L5CX_WrMulti(
		VL53L5CX_Platform *p_platform,
		uint16_t RegisterAdress,
		uint8_t *p_values,
		uint32_t size);

/**
 * @brief Optional function, only used to perform an hardware reset of the
 * sensor. Not called by the API.
 */
uint8_t VL53L5CX_Reset_Sensor(
		VL53L5CX_Platform *p_platform);

/**
 * @brief Mandatory function, used to swap a buffer. The buffer size is always a
 * multiple of 4 (4, 8, 12, 16, ...).
 */
void VL53L5CX_SwapBuffer(
		uint8_t 		*buffer,
		uint16_t 	 	 size);

/**
 * @brief Mandatory function, used to wait during an amount of time.
 */
uint8_t VL53L5CX_WaitMs(
		VL53L5CX_Platform *p_platform,
		uint32_t TimeMs);

#endif	// _PLATFORM_H_
