declare void @llvm.stackrestore(i8*)
declare i8* @llvm.stacksave()

define i8* @stack_get() naked optnone {
    %ptr = call i8* @llvm.stacksave()
    ret i8* %ptr
}

define void @stack_set(i8* %ptr) naked optnone {
    call void @llvm.stackrestore(i8* %ptr)
    ret void
}
