#ifndef MICRO_HOST_SINGLE_INSTANCE_H
#define MICRO_HOST_SINGLE_INSTANCE_H

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define MICRO_HOST_SINGLE_INSTANCE_EXPORT __declspec(dllexport)
#else
#define MICRO_HOST_SINGLE_INSTANCE_EXPORT __attribute__((visibility("default")))
#endif

/* Tell the user that Micro OS is already running, then hand the foreground
 * back to the instance that owns the window.
 *
 * The desktop host allows only one visible instance. The first launch holds an
 * advisory lock (`<MICROS_HOME>/instance.lock`, default `~/.micros`) and
 * records its PID there; a duplicate calls this instead of creating a window.
 * On macOS it shows a native modal notice and, once that notice is dismissed,
 * activates the process owning the lock. The caller exits afterwards.
 *
 * `owner_pid` is the PID read from the lock file, or 0 when it could not be
 * read: the notice is still shown, but there is no window to activate. Other
 * platforms provide a no-op stub. */
MICRO_HOST_SINGLE_INSTANCE_EXPORT void micro_host_single_instance_warn_and_activate(int owner_pid);

/* Register the primary instance's window for activation requests.
 *
 * `sdl_window` is the SDL_Window * of the one visible host. A duplicate asks
 * the primary to come forward both through LaunchServices (NSRunningApplication)
 * and through a distributed notification; this handler raises the window again
 * in its own process, which also restores a minimized or hidden window. Call
 * once, after the visible window exists; repeated calls only refresh the
 * pointer. No-op stub elsewhere. */
MICRO_HOST_SINGLE_INSTANCE_EXPORT void micro_host_single_instance_observe(void *sdl_window);

#ifdef __cplusplus
}
#endif

#endif
