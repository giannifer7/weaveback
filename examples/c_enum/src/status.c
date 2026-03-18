#include "status.h"

const char *status_as_string(Status s)
{
    switch (s) {
        case STATUS_OK: return "STATUS_OK";
        case STATUS_NOT_FOUND: return "STATUS_NOT_FOUND";
        case STATUS_FORBIDDEN: return "STATUS_FORBIDDEN";
        case STATUS_INTERNAL_ERROR: return "STATUS_INTERNAL_ERROR";
        default: return "<UNKNOWN>";
    }
}
