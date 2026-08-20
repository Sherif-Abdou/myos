.global _start

.section .data
cool_variable:
    .quad 28

.section .text
_start:
    ldr x3, =cool_variable
    ldr x2, [x3]
    mov x0, #1
    mov x1, #2
    svc #27
