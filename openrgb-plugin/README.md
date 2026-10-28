# OpenRGB Native Plugin & Controller: Fantech MK894 Maxpower II

This directory provides native C++ OpenRGB controller and plugin implementation files for the **Fantech MK894 Maxpower II** mechanical keyboard (USB VID `0x36AE`, PID `0xFE9C`).

---

## Architecture

OpenRGB interfaces with the MK894 using clean-room reverse-engineered hardware protocols:
1. **Controller Layer (`FantechMK894Controller.h`, `.cpp`)**:
   - Communicates over USB HID via `hidapi`.
   - Sends 65-byte feature reports (Report ID `0x06`).
   - Handles global lighting modes via Opcode `0x0B`.
   - Handles per-key RGB LED matrix targeting via Opcode `0x14` (`SET_SINGLE_KEY_LED`).
   - Automatically synchronizes dual-pad edge switches (`/?`, `Num 0`, `Space`, `Enter`, `LeftShift`).
   - Paces packet transmissions at ~15ms intervals to prevent MCU SRAM buffer overruns.

2. **OpenRGB RGBController Layer (`RGBController_FantechMK894.h`, `.cpp`)**:
   - Inherits from `RGBController`.
   - Exposes standard 6x21 matrix layout with all 104 individual mechanical switch LEDs.
   - Implements differential dirty-LED tracking: only modified keys are transmitted over USB, achieving responsive live updates.
   - Exposes all 16 hardware lighting modes (Static, Breathing, Wave, Ripple, Rainbow, Neon, etc.) + Direct / Custom mode.

---

## Integration into Upstream OpenRGB

To compile directly within the OpenRGB source tree:
1. Copy `FantechMK894Controller.h`, `FantechMK894Controller.cpp`, `RGBController_FantechMK894.h`, and `RGBController_FantechMK894.cpp` to `OpenRGB/controllers/FantechMK894Controller/`.
2. Register the controller detector in `OpenRGB/controllers/ControllerDetect.cpp`:
   ```cpp
   #define FANTECH_VID 0x36AE
   #define FANTECH_MK894_PID 0xFE9C

   void DetectFantechMK894Controllers(hid_device_info* info, const std::string& name)
   {
       hid_device* dev = hid_open_path(info->path);
       if(dev)
       {
           FantechMK894Controller*     controller = new FantechMK894Controller(dev, info->path);
           RGBController_FantechMK894* rgb_ctrl   = new RGBController_FantechMK894(controller);
           ResourceManager::get()->RegisterRGBController(rgb_ctrl);
       }
   }
   ```
3. Add controller files to `OpenRGB.pro` / `CMakeLists.txt` and rebuild OpenRGB.

---

## Built-in OpenRGB SDK Server (No Rebuild Required)

If you are running the standard official OpenRGB release without custom compilation, you can control the keyboard directly by running the built-in OpenRGB SDK server in `fantech-ctl`:
```bash
sudo ./fantech-rs/target/release/fantech-ctl openrgb-server
```
This spawns a standards-compliant OpenRGB SDK v3 server on port `6742`. Any OpenRGB client or OpenRGB desktop GUI app can connect to `localhost:6742` and immediately control the MK894 matrix!
