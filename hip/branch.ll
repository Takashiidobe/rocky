; ModuleID = 'lifted'
source_filename = "lifted"

define void @lifted(ptr %vgprs, ptr %sgprs, ptr %exec_lo, ptr %exec_hi, ptr %vcc_lo, ptr %vcc_hi, ptr %scc, ptr %m0) {
entry:
  %and_rhs = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %and_rhs1 = load <32 x i32>, ptr %and_rhs, align 4
  %and = and <32 x i32> splat (i32 1), %and_rhs1
  %old_dest = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %old_dest2 = load <32 x i32>, ptr %old_dest, align 4
  %exec = load i32, ptr %exec_lo, align 4
  %lane_mask = bitcast i32 %exec to <32 x i1>
  %and_dest = select <32 x i1> %lane_mask, <32 x i32> %and, <32 x i32> %old_dest2
  %and_dest3 = getelementptr <32 x i32>, ptr %vgprs, i32 1
  store <32 x i32> %and_dest, ptr %and_dest3, align 128
  %cmp_rhs = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %cmp_rhs4 = load <32 x i32>, ptr %cmp_rhs, align 4
  %equal = icmp eq <32 x i32> splat (i32 1), %cmp_rhs4
  %mask = bitcast <32 x i1> %equal to i32
  %exec5 = load i32, ptr %exec_lo, align 4
  %active_mask = and i32 %mask, %exec5
  store i32 %active_mask, ptr %vcc_lo, align 4
  %old_exec = load i32, ptr %exec_lo, align 4
  %and_saveexec_src = load i32, ptr %vcc_lo, align 4
  %and_saveexec_dest = getelementptr i32, ptr %sgprs, i32 0
  store i32 %old_exec, ptr %and_saveexec_dest, align 4
  %next_exec = and i32 %old_exec, %and_saveexec_src
  store i32 %next_exec, ptr %exec_lo, align 4
  %exec_scc = icmp ne i32 %next_exec, 0
  %exec_scc_i32 = zext i1 %exec_scc to i32
  store i32 %exec_scc_i32, ptr %scc, align 4
  %xor_lhs = load i32, ptr %exec_lo, align 4
  %xor_rhs = getelementptr i32, ptr %sgprs, i32 0
  %xor_rhs6 = load i32, ptr %xor_rhs, align 4
  %xor = xor i32 %xor_lhs, %xor_rhs6
  %xor_scc = icmp ne i32 %xor, 0
  %xor_scc_i32 = zext i1 %xor_scc to i32
  %xor_dest = getelementptr i32, ptr %sgprs, i32 0
  store i32 %xor, ptr %xor_dest, align 4
  store i32 %xor_scc_i32, ptr %scc, align 4
  %mul_lhs = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %mul_lhs7 = load <32 x i32>, ptr %mul_lhs, align 4
  %mul_rhs = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %mul_rhs8 = load <32 x i32>, ptr %mul_rhs, align 4
  %mul_lhs_u24 = and <32 x i32> %mul_lhs7, splat (i32 16777215)
  %mul_rhs_u24 = and <32 x i32> %mul_rhs8, splat (i32 16777215)
  %mul = mul <32 x i32> %mul_lhs_u24, %mul_rhs_u24
  %old_dest9 = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %old_dest10 = load <32 x i32>, ptr %old_dest9, align 4
  %exec11 = load i32, ptr %exec_lo, align 4
  %lane_mask12 = bitcast i32 %exec11 to <32 x i1>
  %mul_dest = select <32 x i1> %lane_mask12, <32 x i32> %mul, <32 x i32> %old_dest10
  %mul_dest13 = getelementptr <32 x i32>, ptr %vgprs, i32 1
  store <32 x i32> %mul_dest, ptr %mul_dest13, align 128
  %old_exec14 = load i32, ptr %exec_lo, align 4
  %andn2_saveexec_src = getelementptr i32, ptr %sgprs, i32 0
  %andn2_saveexec_src15 = load i32, ptr %andn2_saveexec_src, align 4
  %andn2_saveexec_dest = getelementptr i32, ptr %sgprs, i32 0
  store i32 %old_exec14, ptr %andn2_saveexec_dest, align 4
  %inverted_exec = xor i32 %old_exec14, -1
  %next_exec16 = and i32 %andn2_saveexec_src15, %inverted_exec
  store i32 %next_exec16, ptr %exec_lo, align 4
  %exec_scc17 = icmp ne i32 %next_exec16, 0
  %exec_scc_i3218 = zext i1 %exec_scc17 to i32
  store i32 %exec_scc_i3218, ptr %scc, align 4
  %shift_value = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %shift_value19 = load <32 x i32>, ptr %shift_value, align 4
  %shift = shl <32 x i32> %shift_value19, splat (i32 1)
  %old_dest20 = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %old_dest21 = load <32 x i32>, ptr %old_dest20, align 4
  %exec22 = load i32, ptr %exec_lo, align 4
  %lane_mask23 = bitcast i32 %exec22 to <32 x i1>
  %shift_dest = select <32 x i1> %lane_mask23, <32 x i32> %shift, <32 x i32> %old_dest21
  %shift_dest24 = getelementptr <32 x i32>, ptr %vgprs, i32 1
  store <32 x i32> %shift_dest, ptr %shift_dest24, align 128
  %or_lhs = load i32, ptr %exec_lo, align 4
  %or_rhs = getelementptr i32, ptr %sgprs, i32 0
  %or_rhs25 = load i32, ptr %or_rhs, align 4
  %or = or i32 %or_lhs, %or_rhs25
  %or_scc = icmp ne i32 %or, 0
  %or_scc_i32 = zext i1 %or_scc to i32
  store i32 %or, ptr %exec_lo, align 4
  store i32 %or_scc_i32, ptr %scc, align 4
  %load_base = getelementptr i64, ptr %sgprs, i32 2
  %load_base26 = load i64, ptr %load_base, align 4
  %load_address = add i64 %load_base26, 0
  %load_ptr = inttoptr i64 %load_address to ptr
  %loaded_pair = load i64, ptr %load_ptr, align 4
  %load_dest = getelementptr i64, ptr %sgprs, i32 0
  store i64 %loaded_pair, ptr %load_dest, align 4
  %shift_value27 = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %shift_value28 = load <32 x i32>, ptr %shift_value27, align 4
  %shift29 = shl <32 x i32> %shift_value28, splat (i32 2)
  %old_dest30 = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %old_dest31 = load <32 x i32>, ptr %old_dest30, align 4
  %exec32 = load i32, ptr %exec_lo, align 4
  %lane_mask33 = bitcast i32 %exec32 to <32 x i1>
  %shift_dest34 = select <32 x i1> %lane_mask33, <32 x i32> %shift29, <32 x i32> %old_dest31
  %shift_dest35 = getelementptr <32 x i32>, ptr %vgprs, i32 0
  store <32 x i32> %shift_dest34, ptr %shift_dest35, align 128
  %store_base = getelementptr i64, ptr %sgprs, i32 0
  %store_base36 = load i64, ptr %store_base, align 4
  %vgpr_low = getelementptr <32 x i32>, ptr %vgprs, i32 0
  %vgpr_low37 = load <32 x i32>, ptr %vgpr_low, align 4
  %vgpr_high = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %vgpr_high38 = load <32 x i32>, ptr %vgpr_high, align 4
  %vgpr_pair_words = shufflevector <32 x i32> %vgpr_low37, <32 x i32> %vgpr_high38, <64 x i32> <i32 0, i32 32, i32 1, i32 33, i32 2, i32 34, i32 3, i32 35, i32 4, i32 36, i32 5, i32 37, i32 6, i32 38, i32 7, i32 39, i32 8, i32 40, i32 9, i32 41, i32 10, i32 42, i32 11, i32 43, i32 12, i32 44, i32 13, i32 45, i32 14, i32 46, i32 15, i32 47, i32 16, i32 48, i32 17, i32 49, i32 18, i32 50, i32 19, i32 51, i32 20, i32 52, i32 21, i32 53, i32 22, i32 54, i32 23, i32 55, i32 24, i32 56, i32 25, i32 57, i32 26, i32 58, i32 27, i32 59, i32 28, i32 60, i32 29, i32 61, i32 30, i32 62, i32 31, i32 63>
  %store_addresses = bitcast <64 x i32> %vgpr_pair_words to <32 x i64>
  %store_values = getelementptr <32 x i32>, ptr %vgprs, i32 1
  %store_values39 = load <32 x i32>, ptr %store_values, align 4
  %store_base_lane = insertelement <32 x i64> zeroinitializer, i64 %store_base36, i32 0
  %store_base_splat = shufflevector <32 x i64> %store_base_lane, <32 x i64> %store_base_lane, <32 x i32> zeroinitializer
  %store_addresses40 = add <32 x i64> %store_base_splat, %store_addresses
  %store_pointers = inttoptr <32 x i64> %store_addresses40 to <32 x ptr>
  %exec41 = load i32, ptr %exec_lo, align 4
  %store_mask = bitcast i32 %exec41 to <32 x i1>
  call void @llvm.masked.scatter.v32i32.v32p0(<32 x i32> %store_values39, <32 x ptr> %store_pointers, <32 x i1> %store_mask)
  ret void
}

; Function Attrs: nocallback nofree nosync nounwind willreturn memory(write)
declare void @llvm.masked.scatter.v32i32.v32p0(<32 x i32>, <32 x ptr>, <32 x i1>) #0

attributes #0 = { nocallback nofree nosync nounwind willreturn memory(write) }
