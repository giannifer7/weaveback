// main.c
// I’d Really Rather You Didn’t edit this generated file.


#include "session.h"

int main() {
    Session s;
    if (session_init(&s) == 0) {
        printf("\nSession initialized. Performing safe teardown...\n\n");
        session_free(&s);
    }
    return 0;
}

