(module
  (import "world" "random" (func $random (result i64)))
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "roll")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i64_u (i64.rem_u (call $random) (i64.const 6))))))
