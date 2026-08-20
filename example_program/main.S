.global _start

.section .data
cool_variable:
    .quad 0x21

.section .text
_start:
    mov x0, #1
    mov x1, #2
    svc #27
