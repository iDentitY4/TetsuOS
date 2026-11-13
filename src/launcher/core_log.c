/* The log interface libretro cores are given (`retro_log_callback`). Some cores call it
 * without checking that the frontend gave one, and Rust can't define a C function that
 * takes variable arguments, so this one is C. It prints warnings and errors to stderr,
 * which goes to the launcher's log, and stops after a few hundred, in case a core warns
 * on every frame. */
#include <stdarg.h>
#include <stdio.h>

/* `RETRO_LOG_WARN`; debug and info messages are left out. */
#define LAUNCHER_LOG_WARN 2
#define LAUNCHER_LOG_LIMIT 300

static unsigned launcher_logged;

void launcher_core_log(int level, const char *format, ...)
{
    va_list args;

    if (level < LAUNCHER_LOG_WARN || format == NULL || launcher_logged >= LAUNCHER_LOG_LIMIT)
        return;
    launcher_logged++;
    fputs(level == LAUNCHER_LOG_WARN ? "core warning: " : "core error: ", stderr);
    va_start(args, format);
    vfprintf(stderr, format, args);
    va_end(args);
    if (launcher_logged == LAUNCHER_LOG_LIMIT)
        fputs("\n(no more core messages)\n", stderr);
}
