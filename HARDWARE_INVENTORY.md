# Hardware Inventory

## Development Boards

| Qty | Item | MCU | Notes |
|-----|------|-----|-------|
| 2 | NUCLEO-F446RE | STM32F446RET6, Cortex-M4F | Used in bacnet-gateway / modbus projects |
| 2 | NUCLEO-WL55JC1 | STM32WL55, Cortex-M4 + SX126x LoRa radio | LoRaWAN sensor nodes; AU915 |
| 1 | B-U585I-IOT02A Discovery Kit | STM32U585, Cortex-M33 | Primary edge AI board; target for occupancy sensor project |

## LoRaWAN Gateway

| Qty | Item | Notes |
|-----|------|-------|
| 1 | WisGate Edge Lite 2 (RAK7268V2) | AU915 band, no LTE, LoRaWAN gateway |

## Sensors

| Qty | Item | Interface | Measurements |
|-----|------|-----------|-------------|
| 1 | VL53L5CX-SATEL (511-VL53L5CX-SATEL) | I2C | 8×8 zone Time-of-Flight; primary sensor for occupancy project |
| 1 | SparkFun Grid-EYE AMG8833 (SEN-14607) | I2C | 8×8 thermopile IR array; detects radiated heat per pixel |
| 1 | BME680 Environmental Module | I2C | Temp, humidity, pressure, VOC/gas |
| 2 | SHT31-D Temperature & Humidity | I2C | Temp, humidity; two selectable addresses |
| 2 | Adafruit SHT41 | I2C (STEMMA QT/Qwiic) | Temp, humidity |
| 2 | BMP280 | I2C/SPI | Temp, atmospheric pressure; 3.3V |

## Connectivity

| Qty | Item | Interface | Notes |
|-----|------|-----------|-------|
| 2 | W5500 Ethernet Module | SPI | TCP/IP offload, 3.3V/5V; used in Modbus/BACnet nodes |
| 1 | Ethernet Switch | — | OT subnet (10.10.10.x) |

## Displays

| Qty | Item | Interface | Resolution | Notes |
|-----|------|-----------|-----------|-------|
| 3 | OLED Module | I2C | 128×32, white | |
| 2 | SH1106 OLED | I2C | 128×64 | |
| 1 | HiLetgo ILI9341 TFT LCD | SPI | 240×320, 2.8" | Touch panel; on order |

## Test & Debug Equipment

| Qty | Item | Notes |
|-----|------|-------|
| 1 | DIGISHUO USB Logic Analyzer | 24 MHz, 8-channel; UART/I2C/SPI decode |

## Prototyping

| Qty | Item | Notes |
|-----|------|-------|
| 2 | ELEGOO Electronics Kit | 830-point breadboard, power supply module, jumper wires, potentiometer |

## Compute

| Item | Spec | OS |
|------|------|----|
| Laptop | 480s CPU, 12 GB RAM | Pop OS 24.04 |
| Desktop | AMD Ryzen 5600G, 32 GB RAM | Pop OS 24.04 |

---

## Notes

- **ILI9341** is on order — colour touchscreen, good candidate for a local inference result display on the occupancy sensor node.
- **BME688** (x2 Fermion, onboard BSEC2 AI) — currently misplaced; not blocking any active project. Hold off on reordering until needed.
- LoRaWAN is handled exclusively by the **STM32WL55JC1** nodes + RAK7268V2 gateway (AU915).
