/*-----------------------------------------*\
|  RGBController_FantechMK894.h             |
|                                           |
|  OpenRGB RGBController implementation for |
|  Fantech MK894 Maxpower II keyboard       |
\*-----------------------------------------*/

#pragma once

#include "RGBController.h"
#include "FantechMK894Controller.h"
#include <vector>

class RGBController_FantechMK894 : public RGBController
{
public:
    RGBController_FantechMK894(FantechMK894Controller* controller_ptr);
    ~RGBController_FantechMK894();

    void SetupZones();
    void ResizeZone(int zone, int new_size);

    void DeviceUpdateLEDs();
    void UpdateZoneLEDs(int zone);
    void UpdateSingleLED(int led);

    void DeviceUpdateMode();

private:
    FantechMK894Controller* controller;
    std::vector<RGBColor>   last_colors;
};
