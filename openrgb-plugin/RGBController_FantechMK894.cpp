/*-----------------------------------------*\
|  RGBController_FantechMK894.cpp           |
|                                           |
|  OpenRGB RGBController implementation for |
|  Fantech MK894 Maxpower II keyboard       |
\*-----------------------------------------*/

#include "RGBController_FantechMK894.h"
#include <cstring>

struct KeyMatrixPos
{
    const char*  name;
    unsigned int matrix_index; // Physical MK894 MCU sparse index (0..126)
    unsigned int row;
    unsigned int col;
};

// 104 keys of the MK894 with exact physical matrix coordinates
static const KeyMatrixPos MK894_KEYS[] =
{
    // Row 0
    { "Key: Escape",              0, 0,  0 },
    { "Key: F1",                  1, 0,  1 },
    { "Key: F2",                  2, 0,  2 },
    { "Key: F3",                  3, 0,  3 },
    { "Key: F4",                  4, 0,  4 },
    { "Key: F5",                  5, 0,  5 },
    { "Key: F6",                  6, 0,  6 },
    { "Key: F7",                  7, 0,  7 },
    { "Key: F8",                  8, 0,  8 },
    { "Key: F9",                  9, 0,  9 },
    { "Key: F10",                10, 0, 10 },
    { "Key: F11",                11, 0, 11 },
    { "Key: F12",                12, 0, 12 },
    { "Key: Print Screen",       13, 0, 13 },
    { "Key: Scroll Lock",        14, 0, 14 },
    { "Key: Pause/Break",        15, 0, 15 },

    // Row 1
    { "Key: ` ~",                21, 1,  0 },
    { "Key: 1 !",                22, 1,  1 },
    { "Key: 2 @",                23, 1,  2 },
    { "Key: 3 #",                24, 1,  3 },
    { "Key: 4 $",                25, 1,  4 },
    { "Key: 5 %",                26, 1,  5 },
    { "Key: 6 ^",                27, 1,  6 },
    { "Key: 7 &",                28, 1,  7 },
    { "Key: 8 *",                29, 1,  8 },
    { "Key: 9 (",                30, 1,  9 },
    { "Key: 0 )",                31, 1, 10 },
    { "Key: - _",                32, 1, 11 },
    { "Key: = +",                33, 1, 12 },
    { "Key: Backspace",          34, 1, 13 },
    { "Key: Insert",             35, 1, 14 },
    { "Key: Home",               36, 1, 15 },
    { "Key: Page Up",            37, 1, 16 },
    { "Key: Num Lock",           38, 1, 17 },
    { "Key: Num /",              39, 1, 18 },
    { "Key: Num *",              40, 1, 19 },
    { "Key: Num -",              41, 1, 20 },

    // Row 2
    { "Key: Tab",                42, 2,  0 },
    { "Key: Q",                  43, 2,  1 },
    { "Key: W",                  44, 2,  2 },
    { "Key: E",                  45, 2,  3 },
    { "Key: R",                  46, 2,  4 },
    { "Key: T",                  47, 2,  5 },
    { "Key: Y",                  48, 2,  6 },
    { "Key: U",                  49, 2,  7 },
    { "Key: I",                  50, 2,  8 },
    { "Key: O",                  51, 2,  9 },
    { "Key: P",                  52, 2, 10 },
    { "Key: [ {",                53, 2, 11 },
    { "Key: ] }",                54, 2, 12 },
    { "Key: \\ |",               55, 2, 13 },
    { "Key: Delete",             56, 2, 14 },
    { "Key: End",                57, 2, 15 },
    { "Key: Page Down",          58, 2, 16 },
    { "Key: Num 7",              59, 2, 17 },
    { "Key: Num 8",              60, 2, 18 },
    { "Key: Num 9",              61, 2, 19 },
    { "Key: Num +",              62, 2, 20 },

    // Row 3
    { "Key: Caps Lock",          63, 3,  0 },
    { "Key: A",                  64, 3,  1 },
    { "Key: S",                  65, 3,  2 },
    { "Key: D",                  66, 3,  3 },
    { "Key: F",                  67, 3,  4 },
    { "Key: G",                  68, 3,  5 },
    { "Key: H",                  69, 3,  6 },
    { "Key: J",                  70, 3,  7 },
    { "Key: K",                  71, 3,  8 },
    { "Key: L",                  72, 3,  9 },
    { "Key: ; :",                73, 3, 10 },
    { "Key: ' \"",               74, 3, 11 },
    { "Key: Enter",              76, 3, 13 },
    { "Key: Num 4",              80, 3, 17 },
    { "Key: Num 5",              81, 3, 18 },
    { "Key: Num 6",              82, 3, 19 },

    // Row 4
    { "Key: Left Shift",         84, 4,  0 },
    { "Key: Z",                  86, 4,  2 },
    { "Key: X",                  87, 4,  3 },
    { "Key: C",                  88, 4,  4 },
    { "Key: V",                  89, 4,  5 },
    { "Key: B",                  90, 4,  6 },
    { "Key: N",                  91, 4,  7 },
    { "Key: M",                  92, 4,  8 },
    { "Key: , <",                93, 4,  9 },
    { "Key: . >",                94, 4, 10 },
    { "Key: / ?",                96, 4, 11 },
    { "Key: Right Shift",        97, 4, 13 },
    { "Key: Up Arrow",           99, 4, 15 },
    { "Key: Num 1",             101, 4, 17 },
    { "Key: Num 2",             102, 4, 18 },
    { "Key: Num 3",             103, 4, 19 },
    { "Key: Num Enter",         104, 4, 20 },

    // Row 5
    { "Key: Left Control",      105, 5,  0 },
    { "Key: Left Windows",      106, 5,  1 },
    { "Key: Left Alt",          107, 5,  2 },
    { "Key: Space",             111, 5,  6 },
    { "Key: Right Alt",         115, 5, 10 },
    { "Key: Function",          116, 5, 11 },
    { "Key: Menu / App",        117, 5, 12 },
    { "Key: Right Control",     118, 5, 13 },
    { "Key: Left Arrow",        119, 5, 14 },
    { "Key: Down Arrow",        120, 5, 15 },
    { "Key: Right Arrow",       121, 5, 16 },
    { "Key: Num 0",             122, 5, 18 },
    { "Key: Num .",             124, 5, 19 }
};

static const unsigned int NUM_KEYS = sizeof(MK894_KEYS) / sizeof(MK894_KEYS[0]);

RGBController_FantechMK894::RGBController_FantechMK894(FantechMK894Controller* controller_ptr)
{
    controller  = controller_ptr;

    name        = "Fantech MK894 Maxpower II";
    vendor      = "Fantech";
    type        = DEVICE_TYPE_KEYBOARD;
    description = "Fantech MK894 Maxpower II Mechanical Keyboard";
    location    = controller->GetLocation();
    serial      = controller->GetSerial();

    // Mode 0: Direct / Custom matrix per-key control
    mode CustomMode;
    CustomMode.name       = "Direct / Custom";
    CustomMode.value      = 0x10; // Hardware Mode 16
    CustomMode.flags      = MODE_FLAG_HAS_PER_LED_COLOR;
    CustomMode.color_mode = MODE_COLORS_PER_LED;
    modes.push_back(CustomMode);

    // Hardware Animation Modes
    const struct { const char* name; unsigned char val; unsigned int flags; } hw_modes[] = {
        { "Static",          0x00, MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_BRIGHTNESS },
        { "Breathing",       0x01, MODE_FLAG_HAS_MODE_SPECIFIC_COLOR | MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Neon",            0x02, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Wave",            0x03, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_DIRECTION_LR },
        { "Ripple",          0x04, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Raindrop",        0x05, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Snake",           0x06, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Trigger",         0x07, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS | MODE_FLAG_HAS_MODE_SPECIFIC_COLOR },
        { "Audio Visualizer",0x08, MODE_FLAG_HAS_BRIGHTNESS },
        { "Sine Wave",       0x09, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Cross",           0x0A, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Diagonal Wave",   0x0B, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Center Ripple",   0x0C, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Kaleidoscope",    0x0D, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Scanline",        0x0E, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
        { "Radar",           0x0F, MODE_FLAG_HAS_SPEED | MODE_FLAG_HAS_BRIGHTNESS },
    };

    for(const auto& m : hw_modes)
    {
        mode ModeEntry;
        ModeEntry.name             = m.name;
        ModeEntry.value            = m.val;
        ModeEntry.flags            = m.flags;
        ModeEntry.color_mode       = (m.flags & MODE_FLAG_HAS_MODE_SPECIFIC_COLOR) ? MODE_COLORS_MODE_SPECIFIC : MODE_COLORS_NONE;
        ModeEntry.brightness_min   = 0;
        ModeEntry.brightness_max   = 4;
        ModeEntry.brightness       = 4;
        ModeEntry.speed_min        = 0;
        ModeEntry.speed_max        = 4;
        ModeEntry.speed            = 2;
        if(m.flags & MODE_FLAG_HAS_MODE_SPECIFIC_COLOR)
        {
            ModeEntry.colors.resize(1);
            ModeEntry.colors[0] = ToRGBColor(255, 0, 0);
        }
        modes.push_back(ModeEntry);
    }

    SetupZones();
}

RGBController_FantechMK894::~RGBController_FantechMK894()
{
    delete controller;
}

void RGBController_FantechMK894::SetupZones()
{
    zone keyboard_zone;
    keyboard_zone.name                   = "Keyboard";
    keyboard_zone.type                   = ZONE_TYPE_MATRIX;
    keyboard_zone.leds_min               = NUM_KEYS;
    keyboard_zone.leds_max               = NUM_KEYS;
    keyboard_zone.leds_count             = NUM_KEYS;
    keyboard_zone.matrix_map             = new matrix_map_type;
    keyboard_zone.matrix_map->height     = 6;
    keyboard_zone.matrix_map->width      = 21;
    keyboard_zone.matrix_map->matrix     = new unsigned int[6 * 21];

    // Initialize all matrix cells to unpopulated
    for(unsigned int idx = 0; idx < 6 * 21; ++idx)
    {
        keyboard_zone.matrix_map->matrix[idx] = 0xFFFFFFFF;
    }

    // Populate matrix and LED entries
    for(unsigned int i = 0; i < NUM_KEYS; ++i)
    {
        led new_led;
        new_led.name  = MK894_KEYS[i].name;
        new_led.value = MK894_KEYS[i].matrix_index;
        leds.push_back(new_led);

        unsigned int matrix_slot = (MK894_KEYS[i].row * 21) + MK894_KEYS[i].col;
        if(matrix_slot < 6 * 21)
        {
            keyboard_zone.matrix_map->matrix[matrix_slot] = i;
        }
    }

    zones.push_back(keyboard_zone);

    SetupColors();

    last_colors.resize(colors.size(), 0x00000000);
}

void RGBController_FantechMK894::ResizeZone(int /*zone*/, int /*new_size*/)
{
    // Fixed layout keyboard; resizing not supported
}

void RGBController_FantechMK894::DeviceUpdateLEDs()
{
    std::vector<std::pair<unsigned int, RGBColor>> changes;

    for(std::size_t i = 0; i < colors.size(); ++i)
    {
        if(colors[i] != last_colors[i])
        {
            changes.emplace_back(leds[i].value, colors[i]);
            last_colors[i] = colors[i];
        }
    }

    if(!changes.empty())
    {
        controller->SendMatrixKeys(changes);
    }
}

void RGBController_FantechMK894::UpdateZoneLEDs(int /*zone*/)
{
    DeviceUpdateLEDs();
}

void RGBController_FantechMK894::UpdateSingleLED(int led)
{
    if(led >= 0 && static_cast<std::size_t>(led) < colors.size())
    {
        unsigned char r = RGBGetRValue(colors[led]);
        unsigned char g = RGBGetGValue(colors[led]);
        unsigned char b = RGBGetBValue(colors[led]);

        controller->SetKeyColor(leds[led].value, r, g, b);
        last_colors[led] = colors[led];
    }
}

void RGBController_FantechMK894::DeviceUpdateMode()
{
    if(active_mode >= 0 && static_cast<std::size_t>(active_mode) < modes.size())
    {
        const mode& cur = modes[active_mode];

        unsigned char color_mode = (cur.flags & MODE_FLAG_HAS_MODE_SPECIFIC_COLOR) ? 1 : 0;
        unsigned char r = 255, g = 0, b = 0;
        if(!cur.colors.empty())
        {
            r = RGBGetRValue(cur.colors[0]);
            g = RGBGetGValue(cur.colors[0]);
            b = RGBGetBValue(cur.colors[0]);
        }

        controller->SetLightMode(
            static_cast<unsigned char>(cur.value),
            static_cast<unsigned char>(cur.brightness),
            static_cast<unsigned char>(cur.speed),
            color_mode,
            r, g, b
        );
    }
}
