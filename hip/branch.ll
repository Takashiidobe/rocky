; ModuleID = 'compare'
source_filename = "compare"

define void @lifted(ptr %vgprs, i32 %exec, ptr %vcc_lo) {
entry:
  %rhs = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %rhs1 = load <32 x i32>, ptr %rhs, align 4
  %equal = icmp eq <32 x i32> splat (i32 1), %rhs1
  %mask = bitcast <32 x i1> %equal to i32
  %active_mask = and i32 %mask, %exec
  store i32 %active_mask, ptr %vcc_lo, align 4
  ret void
}
