!macro NSIS_HOOK_PREINSTALL
  ; 检测 VC++ 2015-2022 Redistributable (x64)
  ; 注册表路径: HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64
  ; 同时检查 Builtin 和 Redistributable 两种注册方式
  StrCpy $1 "0"
  ReadRegDWORD $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
  ${If} $0 == "1"
    StrCpy $1 "1"
  ${EndIf}
  ; 备用检测: WinSxS 组件 (VS 2022 / 14.3x)
  ${If} $1 == "0"
    ClearErrors
    EnumRegKey $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" ""
    ${IfNot} ${Errors}
      StrCpy $1 "1"
    ${EndIf}
  ${EndIf}
  ${If} $1 == "0"
    MessageBox MB_YESNO "需要安装 Visual C++ 2015-2022 运行时 (x64)，是否现在安装？" IDYES install_vcredist IDNO abort_install
    install_vcredist:
      DetailPrint "正在安装 VC++ Redistributable..."
      ExecWait '"$INSTDIR\vc_redist.x64.exe" /quiet /norestart' $0
      ${If} $0 != "0"
        MessageBox MB_OK "VC++ Redistributable 安装失败（错误码: $0），请手动安装。"
        Abort
      ${EndIf}
      Goto done_vcredist
    abort_install:
      Abort "安装已取消。VC++ 运行时是必需组件。"
    done_vcredist:
  ${EndIf}
!macroend
