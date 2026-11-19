# OpenRGB Integration Blueprint: Fantech MK894 Maxpower II

**Target:** Fantech MK894 Maxpower II (Profile `AK107`)  
**Author:** Ahmad Serieh  
**Status:** Architecture Blueprint for Upstream PR  
**USB Identifiers:**
* **Vendor ID (VID)**: `0x36AE`
* **Product ID (PID)**: `0xFE9C` (Primary MK894) / `0xFEAB` (Secondary)
* **Usage Page**: `0xFF00`
* **Usage**: `0x0002`

---

## 1. Controller Class Architecture

In OpenRGB's source hierarchy (`Controllers/KeyboardControllers/`):
* Header: `FantechMK894Controller.h`
* Implementation: `FantechMK894Controller.cpp`
* Detection: Register in `RGBController_FantechMK894Detect.cpp`

### Controller Declaration (`FantechMK894Controller.h`)
```cpp
#pragma once

#include "RGBController.h"
#include <hidapi/hidapi.h>

#define FANTECH_MK894_REPORT_ID     0x06
#define FANTECH_MK894_REPORT_SIZE   65

#define FANTECH_MK894_OP_SET_LIGHT  0x0B
#define FANTECH_MK894_OP_SET_BATCH  0x12
#define FANTECH_MK894_OP_SET_SINGLE 0x14

class FantechMK894Controller
{
public:
    FantechMK894Controller(hid_device* dev_handle, const char* path);
    ~FantechMK894Controller();

    std::string GetLocation();
    std::string GetSerial();

    void SetMode(unsigned char mode, unsigned char brightness, unsigned char speed, unsigned char color_mode, unsigned char h, unsigned char s, unsigned char v);
    void SetKeyColor(unsigned int key_index, unsigned char r, unsigned char g, unsigned char b);
    void SendColorMatrix(const std::vector<RGBColor>& colors);

private:
    hid_device* dev;
    std::string location;
};
```

---

## 2. Packet Transmission Implementation

```cpp
#include "FantechMK894Controller.h"
#include <cstring>

FantechMK894Controller::FantechMK894Controller(hid_device* dev_handle, const char* path)
{
    dev = dev_handle;
    location = path;
}

FantechMK894Controller::~FantechMK894Controller()
{
    if(dev)
    {
        hid_close(dev);
    }
}

void FantechMK894Controller::SetMode(unsigned char mode, unsigned char brightness, unsigned char speed, unsigned char color_mode, unsigned char h, unsigned char s, unsigned char v)
{
    unsigned char buf[FANTECH_MK894_REPORT_SIZE] = { 0 };

    buf[0]  = FANTECH_MK894_REPORT_ID;     // 0x06
    buf[1]  = FANTECH_MK894_OP_SET_LIGHT;  // 0x0B
    buf[2]  = 0x0B;                        // Length = 11 bytes
    buf[3]  = 0x00;
    buf[4]  = 0x00;
    buf[5]  = 0x00;                        // Type = 0 (Main keyboard)
    buf[6]  = 0x00;
    buf[7]  = mode;                        // 0..16
    buf[8]  = brightness;                  // 0..100
    buf[9]  = speed;                       // 0..100
    buf[10] = 0x00;                        // Direction
    buf[11] = color_mode;                  // 0 = Spectrum, 1 = Single color
    buf[12] = 0x00;
    buf[13] = h;                           // Hue (0..255)
    buf[14] = s;                           // Saturation (0..255)
    buf[15] = v;                           // Value (0..255)

    hid_write(dev, buf, FANTECH_MK894_REPORT_SIZE);
}

void FantechMK894Controller::SetKeyColor(unsigned int key_index, unsigned char r, unsigned char g, unsigned char b)
{
    unsigned char buf[FANTECH_MK894_REPORT_SIZE] = { 0 };
    unsigned int offset = key_index * 3;

    buf[0]  = FANTECH_MK894_REPORT_ID;     // 0x06
    buf[1]  = FANTECH_MK894_OP_SET_SINGLE; // 0x14
    buf[2]  = 0x03;                        // Sub-length 3 (R, G, B)
    buf[3]  = offset & 0xFF;               // Offset LSB
    buf[4]  = (offset >> 8) & 0xFF;        // Offset MSB
    buf[5]  = 0x00;
    buf[6]  = 0x00;
    buf[7]  = 0x00;
    buf[8]  = r;
    buf[9]  = g;
    buf[10] = b;

    hid_write(dev, buf, FANTECH_MK894_REPORT_SIZE);
}
```

---

## 3. OpenRGB Mode Mapping

| OpenRGB Mode | Hardware Mode ID | Flags Supported |
| :--- | :--- | :--- |
| **Off** | `0` | None |
| **Direct / Static** | `1` | Brightness, Color |
| **Breathing** | `2` | Brightness, Speed, Color |
| **Spectrum Cycle** | `4` | Brightness, Speed |
| **Rainbow Wave** | `5` | Brightness, Speed, Direction |
| **Reactive** | `13` | Brightness, Speed, Color |
| **Custom (Per-Key)** | `16` | Per-key Color Matrix (`0x12` / `0x14`) |
