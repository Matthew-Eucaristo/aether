(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "open")
  (func (export "tick")
    (call $set (i32.const 1) (i32.const 0) (i32.const 4) (f64.const 1))))
