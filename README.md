# TodoESP
**Keep track of your Todoist tasks on an ESP32+ePaper display.**

This project is designed to run on an ESP32 micro-controller with a
[Waveshare 5.65" e-Paper Module (F)](https://www.waveshare.com/product/5.65inch-e-paper-module-f.htm).
When configured correctly, it will automatically connect to your WiFi and fetch the daily items
on your Todoist task list, showing them on the display. Items will be refreshed every 5 minutes
and the display will be updated when they change (avoiding unnecessary screen refreshes).
