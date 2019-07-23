declare void @llvm.eh.sjlj.longjmp(i8*)
declare i32 @llvm.eh.sjlj.setjmp(i8*)
declare void @llvm.stackrestore(i8*)
declare i8* @llvm.frameaddress(i32)
declare i8* @llvm.stacksave()

; This function is essentially what __builtin_setjmp() emits in C.
; We put it here and mark it as alwaysinline for code-reuse.
; This function is internal only.
define private i32
@jump_save(i8** nonnull %ctx)
alwaysinline nounwind naked
{
  ; Store the frame address.
  %frame = call i8* @llvm.frameaddress(i32 0)
  %foff = getelementptr inbounds i8*, i8** %ctx, i32 0
  store i8* %frame, i8** %foff

  ; Store the stack address.
  %stack = call i8* @llvm.stacksave()
  %soff = getelementptr inbounds i8*, i8** %ctx, i32 2
  store i8* %stack, i8** %soff

  ; The rest are architecture specific and stored by setjmp().
  %buff = bitcast i8** %ctx to i8*
  %retv = call i32 @llvm.eh.sjlj.setjmp(i8* %buff)
  ret i32 %retv
}

; This function is essentially what __builtin_longjmp() emits in C.
; The purpose is to expose this intrinsic to Rust (without requiring nightly).
define dso_local void
@jump_into(i8** %into)
noreturn nounwind naked
{
  %buff = bitcast i8** %into to i8*
  call void @llvm.eh.sjlj.longjmp(i8* %buff) ; Call longjmp()
  unreachable
}

; This function performs a bidirectional context switch.
; It simply calls setjmp(%from) and then longjmp(%into).
define dso_local void
@jump_swap(i8** %from, i8** %into)
nounwind
{
  %retv = call i32 @jump_save(i8** %from)    ; setjmp(%from)
  %zero = icmp eq i32 %retv, 0
  br i1 %zero, label %jump, label %done

jump:                                        ; setjmp(%from) returned 0
  %ibuf = bitcast i8** %into to i8*
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
@jump_init(i8* %addr, i8* %c, i8* %f, void (i8**, i8*, i8*)* %func)
nounwind
{

  %buff = alloca [5 x i8*]                    ; Allocate setjmp() buffer

  %cast = bitcast [5 x i8*]* %buff to i8**
  %retv = call i32 @jump_save(i8** %cast)     ; Call setjmp(%buff)
  %zero = icmp eq i32 %retv, 0
  br i1 %zero, label %next, label %done

next:                                         ; setjmp(%buff) returned 0
  call void @llvm.stackrestore(i8* %addr)     ; Move onto new stack %addr
  call void %func(i8** %cast, i8* %c, i8* %f) ; Call %func(%buff, %c, %f)
  unreachable

done:                                         ; setjmp(%buff) returned !0
  ret void
}
