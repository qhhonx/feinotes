#include <stdbool.h>
#include <stdint.h>

bool fei_ipad_start(const char *library);
bool fei_ipad_prepare_cloud(const char *library, const char *channel);
void fei_ipad_flush(void);
void fei_ipad_shutdown(void);
void *gpui_ios_get_window(void);
void gpui_ios_show_keyboard(void *window);
void *gpui_ios_view_controller(void *window);
void gpui_ios_layout_view(void *window);
bool gpui_ios_request_frame(void *window);
void gpui_ios_did_become_active(void *app);
void gpui_ios_will_resign_active(void *app);

void fei_ios_command(const char *json);
uint64_t fei_ios_publish_navigation_after_pop(void);
void fei_ios_set_editor_input(bool active, const char *scope);
void fei_ios_presence_scene_active(bool active);

bool fei_ios_configure_library(const char *base, const char *channel);

bool fei_ios_prepare_local(void);

typedef void (*FeiPrintCompletion)(int32_t status, const char *message, const void *context);
void fei_ios_print_begin(const char *json, FeiPrintCompletion completion, const void *context);

typedef void (*FeiLibraryCompletion)(int32_t status, const char *message, const void *context);
void fei_ios_pick_library(const char *title, FeiLibraryCompletion completion, const void *context);

// Coordinates are in the GPUI editor view's logical points.
bool fei_ios_horizontal_scroll_at(float x, float y);
bool fei_ios_qa_horizontal_scroll(uint8_t kind, float out[5]);

// UIKit owns interactive vertical scroll on the iPhone document surface.
void fei_ios_enable_native_scroll(bool enabled);
void fei_ios_scroll_editor(float delta, bool top);
bool fei_ios_native_scroll_metrics(float out[2]);
bool fei_ios_native_scroll_reserved(void);
