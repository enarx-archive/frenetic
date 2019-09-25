declare void @llvm.eh.sjlj.longjmp(i8*) noreturn nounwind
declare i32 @llvm.eh.sjlj.setjmp(i8*) nounwind
declare void @llvm.stackrestore(i8*) nounwind
declare i8* @llvm.frameaddress(i32) nounwind readnone
declare i8* @llvm.stacksave() nounwind

; We put it here and mark it as alwaysinline for code-reuse.
; This function is internal only.
define private i32
@jump_save([5 x i8*]* nonnull %ctx)
alwaysinline nounwind naked returns_twice
{
  ; Store the frame address.
  %frame = call i8* @llvm.frameaddress(i32 0)
  %foff = getelementptr inbounds [5 x i8*], [5 x i8*]* %ctx, i64 0, i32 0
  store i8* %frame, i8** %foff

  ; Store the stack address.
  %stack = call i8* @llvm.stacksave()
  %soff = getelementptr inbounds [5 x i8*], [5 x i8*]* %ctx, i64 0, i32 2
  store i8* %stack, i8** %soff

  ; The rest are architecture specific and stored by setjmp().
  %buff = bitcast [5 x i8*]* %ctx to i8*
  %retv = call i32 @llvm.eh.sjlj.setjmp(i8* %buff) returns_twice
  ret i32 %retv
}

; This function is essentially what __builtin_longjmp() emits in C.
; The purpose is to expose this intrinsic to Rust (without requiring nightly).
define dso_local void
@jump_into([5 x i8*]* nonnull %into)
noreturn nounwind naked
{
  %buff = bitcast [5 x i8*]* %into to i8*
  call void @llvm.eh.sjlj.longjmp(i8* %buff) ; Call longjmp()
  unreachable
}

; This function performs a bidirectional context switch.
; It simply calls setjmp(%from) and then longjmp(%into).
define dso_local void
@jump_swap([5 x i8*]* nonnull %from, [5 x i8*]* nonnull %into)
nounwind
{
  ; setjmp(%from)
  %retv = call i32 @jump_save([5 x i8*]* %from) returns_twice
  %zero = icmp eq i32 %retv, 0
  br i1 %zero, label %jump, label %done

jump:                                        ; setjmp(%from) returned 0
  %ibuf = bitcast [5 x i8*]* %into to i8*
  call void @llvm.eh.sjlj.longjmp(i8* %ibuf) ; longjmp(%into)
  unreachable

done:                                        ; setjmp(%from) returned !0
  ret void
}

; This function sets up the coroutine. It does the following steps:
;   1. Call setjmp().
;   2. Set the stack pointer to %addr.
;   3. Call %func(%c, %f).
define dso_local void
@jump_init(i8* %addr, i8* %c, i8* %f, void ([5 x i8*]*, i8*, i8*)* %func)
nounwind
{
  %buff = alloca [5 x i8*], align 4          ; Allocate setjmp() buffer

  ; Call setjmp(%buff)
  %retv = call i32 @jump_save([5 x i8*]* %buff) returns_twice
  %zero = icmp eq i32 %retv, 0
  br i1 %zero, label %next, label %done

next:                                         ; setjmp(%buff) returned 0
  call void @llvm.stackrestore(i8* %addr)     ; Move onto new stack %addr
  call void %func([5 x i8*]* %buff, i8* %c, i8* %f) ; Call %func(%buff, %c, %f)
  unreachable

done:                                         ; setjmp(%buff) returned !0
  ret void
}
