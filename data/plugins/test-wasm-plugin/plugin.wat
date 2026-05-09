;; Test WASM plugin implementing the BetterNTE WASM Plugin ABI.
;; This is a minimal test plugin with "add" and "greet" methods.
;; Since JSON parsing in raw WAT is impractical, methods return
;; hardcoded responses to validate the host-side loading mechanism.

(module
  ;; 1 page = 64KB linear memory
  (memory (export "memory") 1)

  ;; Simple bump allocator starting at address 1024
  (global $heap_ptr (mut i32) (i32.const 1024))

  ;; ─── __alloc(size: i32) -> i32 ───
  (func $__alloc (param $size i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap_ptr))
    (global.set $heap_ptr (i32.add (global.get $heap_ptr) (local.get $size)))
    (local.get $ptr)
  )

  ;; ─── __plugin_info() -> i64 ───
  ;; Returns pointer and length of JSON info string packed in i64.
  ;; Info: {"methods":["add","greet"]}
  (func $__plugin_info (result i64)
    ;; Write JSON at address 0:
    ;; {"methods":["add","greet"]}
    (i32.store8 (i32.const  0) (i32.const 123))  ;; {
    (i32.store8 (i32.const  1) (i32.const  34))  ;; "
    (i32.store8 (i32.const  2) (i32.const 109))  ;; m
    (i32.store8 (i32.const  3) (i32.const 101))  ;; e
    (i32.store8 (i32.const  4) (i32.const 116))  ;; t
    (i32.store8 (i32.const  5) (i32.const 104))  ;; h
    (i32.store8 (i32.const  6) (i32.const 111))  ;; o
    (i32.store8 (i32.const  7) (i32.const 100))  ;; d
    (i32.store8 (i32.const  8) (i32.const 115))  ;; s
    (i32.store8 (i32.const  9) (i32.const  34))  ;; "
    (i32.store8 (i32.const 10) (i32.const  58))  ;; :
    (i32.store8 (i32.const 11) (i32.const  91))  ;; [
    (i32.store8 (i32.const 12) (i32.const  34))  ;; "
    (i32.store8 (i32.const 13) (i32.const  97))  ;; a
    (i32.store8 (i32.const 14) (i32.const 100))  ;; d
    (i32.store8 (i32.const 15) (i32.const 100))  ;; d
    (i32.store8 (i32.const 16) (i32.const  34))  ;; "
    (i32.store8 (i32.const 17) (i32.const  44))  ;; ,
    (i32.store8 (i32.const 18) (i32.const  34))  ;; "
    (i32.store8 (i32.const 19) (i32.const 103))  ;; g
    (i32.store8 (i32.const 20) (i32.const 114))  ;; r
    (i32.store8 (i32.const 21) (i32.const 101))  ;; e
    (i32.store8 (i32.const 22) (i32.const 101))  ;; e
    (i32.store8 (i32.const 23) (i32.const 116))  ;; t
    (i32.store8 (i32.const 24) (i32.const  34))  ;; "
    (i32.store8 (i32.const 25) (i32.const  93))  ;; ]
    (i32.store8 (i32.const 26) (i32.const 125))  ;; }

    ;; Return (ptr=0 << 32) | len=27
    (i64.const 27)
  )

  ;; ─── Helper: check if method name is "add" (length 3) ───
  (func $is_add (param $ptr i32) (param $len i32) (result i32)
    (if (result i32) (i32.ne (local.get $len) (i32.const 3))
      (then (i32.const 0))
      (else
        (i32.and
          (i32.and
            (i32.eq (i32.load8_u (local.get $ptr))               (i32.const 97))  ;; 'a'
            (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 1))) (i32.const 100)) ;; 'd'
          )
          (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 2))) (i32.const 100)) ;; 'd'
        )
      )
    )
  )

  ;; ─── Helper: check if method name is "greet" (length 5) ───
  (func $is_greet (param $ptr i32) (param $len i32) (result i32)
    (if (result i32) (i32.ne (local.get $len) (i32.const 5))
      (then (i32.const 0))
      (else
        (i32.and
          (i32.and
            (i32.and
              (i32.and
                (i32.eq (i32.load8_u (local.get $ptr))               (i32.const 103)) ;; 'g'
                (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 1))) (i32.const 114)) ;; 'r'
              )
              (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 2))) (i32.const 101)) ;; 'e'
            )
            (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 3))) (i32.const 101)) ;; 'e'
          )
          (i32.eq (i32.load8_u (i32.add (local.get $ptr) (i32.const 4))) (i32.const 116)) ;; 't'
        )
      )
    )
  )

  ;; ─── __plugin_call(name_ptr, name_len, args_ptr, args_len) -> i64 ───
  (func $__plugin_call (param $name_ptr i32) (param $name_len i32) (param $args_ptr i32) (param $args_len i32) (result i64)
    ;; Check for "add" method
    (if (call $is_add (local.get $name_ptr) (local.get $name_len))
      (then
        ;; Return {"result":42} at address 512
        (i32.store8 (i32.const 512) (i32.const 123))  ;; {
        (i32.store8 (i32.const 513) (i32.const  34))  ;; "
        (i32.store8 (i32.const 514) (i32.const 114))  ;; r
        (i32.store8 (i32.const 515) (i32.const 101))  ;; e
        (i32.store8 (i32.const 516) (i32.const 115))  ;; s
        (i32.store8 (i32.const 517) (i32.const 117))  ;; u
        (i32.store8 (i32.const 518) (i32.const 108))  ;; l
        (i32.store8 (i32.const 519) (i32.const 116))  ;; t
        (i32.store8 (i32.const 520) (i32.const  34))  ;; "
        (i32.store8 (i32.const 521) (i32.const  58))  ;; :
        (i32.store8 (i32.const 522) (i32.const  52))  ;; 4
        (i32.store8 (i32.const 523) (i32.const  50))  ;; 2
        (i32.store8 (i32.const 524) (i32.const 125))  ;; }
        ;; Return (512 << 32) | 13
        (return (i64.or (i64.shl (i64.const 512) (i64.const 32)) (i64.const 13)))
      )
    )

    ;; Check for "greet" method
    (if (call $is_greet (local.get $name_ptr) (local.get $name_len))
      (then
        ;; Return {"greeting":"Hello from WASM!"} at address 512
        ;; {"greeting":"Hello from WASM!"} = 30 chars
        (i32.store8 (i32.const 512) (i32.const 123))  ;; {
        (i32.store8 (i32.const 513) (i32.const  34))  ;; "
        (i32.store8 (i32.const 514) (i32.const 103))  ;; g
        (i32.store8 (i32.const 515) (i32.const 114))  ;; r
        (i32.store8 (i32.const 516) (i32.const 101))  ;; e
        (i32.store8 (i32.const 517) (i32.const 101))  ;; e
        (i32.store8 (i32.const 518) (i32.const 116))  ;; t
        (i32.store8 (i32.const 519) (i32.const 105))  ;; i
        (i32.store8 (i32.const 520) (i32.const 110))  ;; n
        (i32.store8 (i32.const 521) (i32.const 103))  ;; g
        (i32.store8 (i32.const 522) (i32.const  34))  ;; "
        (i32.store8 (i32.const 523) (i32.const  58))  ;; :
        (i32.store8 (i32.const 524) (i32.const  34))  ;; "
        (i32.store8 (i32.const 525) (i32.const  72))  ;; H
        (i32.store8 (i32.const 526) (i32.const 101))  ;; e
        (i32.store8 (i32.const 527) (i32.const 108))  ;; l
        (i32.store8 (i32.const 528) (i32.const 108))  ;; l
        (i32.store8 (i32.const 529) (i32.const 111))  ;; o
        (i32.store8 (i32.const 530) (i32.const  32))  ;; (space)
        (i32.store8 (i32.const 531) (i32.const 102))  ;; f
        (i32.store8 (i32.const 532) (i32.const 114))  ;; r
        (i32.store8 (i32.const 533) (i32.const 111))  ;; o
        (i32.store8 (i32.const 534) (i32.const 109))  ;; m
        (i32.store8 (i32.const 535) (i32.const  32))  ;; (space)
        (i32.store8 (i32.const 536) (i32.const  87))  ;; W
        (i32.store8 (i32.const 537) (i32.const  65))  ;; A
        (i32.store8 (i32.const 538) (i32.const  83))  ;; S
        (i32.store8 (i32.const 539) (i32.const  77))  ;; M
        (i32.store8 (i32.const 540) (i32.const  33))  ;; !
        (i32.store8 (i32.const 541) (i32.const  34))  ;; "
        (i32.store8 (i32.const 542) (i32.const 125))  ;; }
        ;; Return (512 << 32) | 31
        (return (i64.or (i64.shl (i64.const 512) (i64.const 32)) (i64.const 31)))
      )
    )

    ;; Unknown method: return {"error":"unknown method"}
    (i32.store8 (i32.const 512) (i32.const 123))  ;; {
    (i32.store8 (i32.const 513) (i32.const  34))  ;; "
    (i32.store8 (i32.const 514) (i32.const 101))  ;; e
    (i32.store8 (i32.const 515) (i32.const 114))  ;; r
    (i32.store8 (i32.const 516) (i32.const 114))  ;; r
    (i32.store8 (i32.const 517) (i32.const 111))  ;; o
    (i32.store8 (i32.const 518) (i32.const 114))  ;; r
    (i32.store8 (i32.const 519) (i32.const  34))  ;; "
    (i32.store8 (i32.const 520) (i32.const  58))  ;; :
    (i32.store8 (i32.const 521) (i32.const  34))  ;; "
    (i32.store8 (i32.const 522) (i32.const 117))  ;; u
    (i32.store8 (i32.const 523) (i32.const 110))  ;; n
    (i32.store8 (i32.const 524) (i32.const 107))  ;; k
    (i32.store8 (i32.const 525) (i32.const 110))  ;; n
    (i32.store8 (i32.const 526) (i32.const 111))  ;; o
    (i32.store8 (i32.const 527) (i32.const 119))  ;; w
    (i32.store8 (i32.const 528) (i32.const 110))  ;; n
    (i32.store8 (i32.const 529) (i32.const  32))  ;; (space)
    (i32.store8 (i32.const 530) (i32.const 109))  ;; m
    (i32.store8 (i32.const 531) (i32.const 101))  ;; e
    (i32.store8 (i32.const 532) (i32.const 116))  ;; t
    (i32.store8 (i32.const 533) (i32.const 104))  ;; h
    (i32.store8 (i32.const 534) (i32.const 111))  ;; o
    (i32.store8 (i32.const 535) (i32.const 100))  ;; d
    (i32.store8 (i32.const 536) (i32.const  34))  ;; "
    (i32.store8 (i32.const 537) (i32.const 125))  ;; }
    ;; Return (512 << 32) | 26
    (i64.or (i64.shl (i64.const 512) (i64.const 32)) (i64.const 26))
  )
)
