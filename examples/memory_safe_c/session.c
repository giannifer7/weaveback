// session.c
// I’d Really Rather You Didn’t edit this generated file.

#include "session.h"

int session_init(Session *self) {
    
    printf("Initialized direct field: user_id\n");
    
    
    self->username = malloc(sizeof(*(self->username)) * 32);
    if (self->username) {
        printf("Allocated pointer field: username (32 units)\n");
    }
    
    
    self->payload = malloc(sizeof(*(self->payload)) * 1024);
    if (self->payload) {
        printf("Allocated pointer field: payload (1024 units)\n");
    }
    
    
    printf("Initialized direct field: payload_len\n");
    
    return 0;
}

void session_free(Session *self) {
    printf("Freeing pointer field: payload\n");
    free(self->payload);
    printf("Freeing pointer field: username\n");
    free(self->username);
}
