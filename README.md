# TodoESP
**Keep track of your Todoist tasks on an ESP32+ePaper display.**

This project is designed to run on an ESP32 micro-controller with a
[Waveshare 5.65" e-Paper Module (F)](https://www.waveshare.com/product/5.65inch-e-paper-module-f.htm).
When configured correctly, it will automatically connect to your WiFi and fetch the daily items
on your Todoist task list, showing them on the display. Items will be refreshed every 5 minutes
and the display will be updated when they change (avoiding unnecessary screen refreshes).

## BOM
- [ESP32 DevKitC](https://www.espressif.com/en/products/devkits/esp32-devkitc/overview) or equivalent ESP32 with 4MB+ of Flash
- [Waveshare 5.65" e-Paper Module (F)](https://www.waveshare.com/product/5.65inch-e-paper-module-f.htm)
- USB-C power supply (5V, 500mA)
- 3D printed case (optional, model will be published soon)

In total, the project should cost somewhere in the range of EUR70-90 depending on where you source your parts.

### Wiring
The e-Paper module connects over SPI to the ESP32, and we're using the following pins:

| ESP32 Pin | e-Paper Pin |
| --------- | ----------- |
| GPIO 23   | DIN         |
| GPIO 18   | CLK         |
| GPIO 5    | CS          |
| GPIO 17   | DC          |
| GPIO 16   | RST         |
| GPIO 4    | BUSY        |

**NOTE** You can modify these in the `src/main.rs` file if you need to, however be aware that not all pins are created equal and some might not work as expected.