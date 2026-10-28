#pragma once

#include "RGBController.h"
#include <hidapi/hidapi.h>
#include <string>
#include <vector>

#define FANTECH_MK894_REPORT_ID         0x06
#define FANTECH_MK894_REPORT_SIZE       65

#define FANTECH_MK894_OP_SET_LIGHT      0x0B
#define FANTECH_MK894_OP_SET_SINGLE_LED 0x14

class FantechMK894Controller
{
public:
    FantechMK894Controller(hid_device* dev_handle, const char* path);
    ~FantechMK894Controller();

    std::string GetLocation();
    std::string GetSerial();

    void SetLightMode(unsigned char mode, unsigned char brightness, unsigned char speed, unsigned char color_mode, unsigned char h, unsigned char s, unsigned char v);
    void SetKeyColor(unsigned int key_index, unsigned char r, unsigned char g, unsigned char b);
    void SendMatrixKeys(const std::vector<std::pair<unsigned int, RGBColor>>& key_colors);

private:
    hid_device* dev;
    std::string location;
    std::string serial;
};
