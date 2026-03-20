/* include/events.h — generated */
#ifndef EVENTS_H
#define EVENTS_H

typedef enum {
    EVT_PLUGIN_LOAD = 0,
    EVT_PLUGIN_UNLOAD = 1,
    EVT_CONFIG_CHANGE = 2,
    EVT__COUNT
} PluginEvent;

/* @reversed slot: cleanup runs in reverse registration order */
static const void (*EVT_CLEANUP[])(void) = {
    cleanup_config_change,
    cleanup_plugin_unload,
    cleanup_plugin_load,
};

#endif /* EVENTS_H */
