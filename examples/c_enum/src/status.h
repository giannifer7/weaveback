#ifndef STATUS_H
#define STATUS_H

typedef enum {
    STATUS_OK = 0,
    STATUS_NOT_FOUND = 404,
    STATUS_FORBIDDEN = 2,
    STATUS_INTERNAL_ERROR = 3,
} Status;

const char *status_as_string(Status s);

#endif /* STATUS_H */
