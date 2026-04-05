// session.h
// I’d Really Rather You Didn’t edit this generated file.
#ifndef SESSION_H
#define SESSION_H

#include <stdio.h>
#include <stdlib.h>
#include <stddef.h>

typedef struct {
    int user_id;
    char *username;
    unsigned char *payload;
    size_t payload_len;
} Session;

int  session_init(Session *self);
void session_free(Session *self);

#endif // SESSION_H
