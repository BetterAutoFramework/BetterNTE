/*
 * Test FFI plugin for BetterNTE.
 *
 * Demonstrates the C ABI contract:
 *   __plugin_info() -> JSON string with method list
 *   __plugin_call(name, args_json) -> JSON result string
 *
 * Build:
 *   gcc -shared -fPIC -o plugin.so plugin.c      # Linux
 *   cc -shared -fPIC -o plugin.dylib plugin.c     # macOS
 *   cl /LD plugin.c /Fe:plugin.dll                # Windows (MSVC)
 */

#include <stdio.h>
#include <string.h>

/* Static buffer for return values — valid until next call. */
static char result_buf[4096];

/* Return plugin info as a JSON string. */
const char* __plugin_info(void) {
    strcpy(result_buf,
        "{\"methods\":[\"add\",\"greet\"],"
        "\"name\":\"Test FFI Plugin\","
        "\"version\":\"1.0.0\"}");
    return result_buf;
}

/* Dispatch a plugin method call. */
const char* __plugin_call(const char* name, const char* args_json) {
    if (strcmp(name, "add") == 0) {
        int a = 0, b = 0;
        sscanf(args_json, "[%d, %d]", &a, &b);
        sprintf(result_buf, "{\"result\": %d}", a + b);
        return result_buf;
    }

    if (strcmp(name, "greet") == 0) {
        char name_buf[256] = {0};
        /* Extract the first string argument between double quotes. */
        const char* start = strchr(args_json, '"');
        if (start) {
            start++;
            const char* end = strchr(start, '"');
            if (end) {
                size_t len = (size_t)(end - start);
                if (len < sizeof(name_buf)) {
                    strncpy(name_buf, start, len);
                    name_buf[len] = '\0';
                }
            }
        }
        sprintf(result_buf, "{\"result\": \"Hello, %s!\"}", name_buf);
        return result_buf;
    }

    strcpy(result_buf, "{\"error\": \"unknown method\"}");
    return result_buf;
}
