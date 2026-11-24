(module
  (import "world" "set_num" (func $set (param i32 i32 i32 f64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "grow")
  (func (export "tick")
    (call $set (i32.const 0) (i32.const 0) (i32.const 4)
      (f64.convert_i32_s (memory.grow (i32.const 1000))))))
