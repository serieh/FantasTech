/*-----------------------------------------*\
|  FantechMK894Controller.cpp               |
|                                           |
|  Driver for Fantech MK894 Maxpower II     |
|  Mechanical Keyboard (Clean-Room Protocol)|
\*-----------------------------------------*/

#include "FantechMK894Controller.h"
#include <cstring>
#include <thread>
#include <chrono>

FantechMK894Controller::FantechMK894Controller(hid_device* dev_handle, const char* path)
{
    dev      = dev_handle;
    location = path ? path : "";
    serial   = "";

    wchar_t serial_buf[256];
    if(dev && hid_get_serial_number_string(dev, serial_buf, 256) >= 0)
    {
        std::wstring ws(serial_buf);
        serial = std::string(ws.begin(), ws.end());
    }
}

FantechMK894Controller::~FantechMK894Controller()
{
    if(dev)
    {
        hid_close(dev);
        dev = nullptr;
    }
}

std::string FantechMK894Controller::GetLocation()
{
    return location;
}

std::string FantechMK894Controller::GetSerial()
{
    return serial;
}

void FantechMK894Controller::SetLightMode(unsigned char mode, unsigned char brightness, unsigned char speed, unsigned char color_mode, unsigned char h, unsigned char s, unsigned char v)
{
    if(!dev)
    {
        return;
    }

    unsigned char buf[FANTECH_MK894_REPORT_SIZE];
    std::memset(buf, 0x00, sizeof(buf));

    buf[0] = FANTECH_MK894_REPORT_ID;         // 0x06
    buf[1] = FANTECH_MK894_OP_SET_LIGHT;      // 0x0B
    buf[2] = 0x00;                            // Zone 0: Main Backlight
    buf[3] = mode;                            // 0x00..0x10
    buf[4] = brightness;                      // 0x00..0x04
    buf[5] = speed;                           // 0x00..0x04
    buf[6] = 0x00;                            // Direction default
    buf[7] = color_mode;                      // 0 = dynamic spectrum, 1 = fixed color
    buf[8] = h;
    buf[9] = s;
    buf[10] = v;

    hid_write(dev, buf, sizeof(buf));
}

void FantechMK894Controller::SetKeyColor(unsigned int key_index, unsigned char r, unsigned char g, unsigned char b)
{
    if(!dev || key_index > 126)
    {
        return;
    }

    unsigned char buf[FANTECH_MK894_REPORT_SIZE];
    std::memset(buf, 0x00, sizeof(buf));

    buf[0] = FANTECH_MK894_REPORT_ID;              // 0x06
    buf[1] = FANTECH_MK894_OP_SET_SINGLE_LED;     // 0x14
    buf[2] = 0x00;
    buf[3] = 0x00;
    buf[4] = static_cast<unsigned char>(key_index);
    buf[5] = r;
    buf[6] = g;
    buf[7] = b;

    hid_write(dev, buf, sizeof(buf));

    // Handle dual-pad edge switches on MK894 PCB
    unsigned char mirror_idx = 0;
    if(key_index == 96) mirror_idx = 95;        // /?
    else if(key_index == 95) mirror_idx = 96;
    else if(key_index == 122) mirror_idx = 123; // Num 0
    else if(key_index == 123) mirror_idx = 122;
    else if(key_index == 111) mirror_idx = 110; // Space
    else if(key_index == 110) mirror_idx = 111;
    else if(key_index == 76) mirror_idx = 75;   // Enter
    else if(key_index == 75) mirror_idx = 76;
    else if(key_index == 84) mirror_idx = 85;   // LeftShift
    else if(key_index == 85) mirror_idx = 84;

    if(mirror_idx > 0)
    {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
        buf[4] = mirror_idx;
        hid_write(dev, buf, sizeof(buf));
    }
}

void FantechMK894Controller::SendMatrixKeys(const std::vector<std::pair<unsigned int, RGBColor>>& key_colors)
{
    if(!dev)
    {
        return;
    }

    for(const auto& pair : key_colors)
    {
        unsigned char r = RGBGetRValue(pair.second);
        unsigned char g = RGBGetGValue(pair.second);
        unsigned char b = RGBGetBValue(pair.second);

        SetKeyColor(pair.first, r, g, b);
        // Pacing: 15ms USB HID packet spacing prevents MCU SRAM buffer overrun
        std::this_thread::sleep_for(std::chrono::milliseconds(15));
    }
}
