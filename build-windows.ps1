#!/usr/bin/env powershell
#
# Windows Release 构建脚本
# 从 .github/workflows/zap_release.yml 的 release_windows job 抽取，
# 用于在本地 Windows 环境构建 oss channel 的 Release 版本。
#
# 用法: .\build-windows.ps1 [-SkipDeps] [-SkipInstaller] [-Arch <x64|arm64>]
#
# author logic
# date 2026/06/02

Param(
    # 跳过依赖安装步骤（已安装好环境时使用）
    [Switch]$SkipDeps = $False,

    # 仅构建二进制，跳过安装包生成
    [Switch]$SkipInstaller = $False,

    # 目标架构，默认自动检测
    [ValidateSet('x64', 'arm64')]
    [String]$Arch = '',

    # Release channel，默认 oss
    [ValidateSet('local', 'dev', 'preview', 'stable', 'oss')]
    [String]$Channel = 'oss'
)

$ErrorActionPreference = 'Stop'

# ── 工具函数 ────────────────────────────────────────────────

function Test-Command {
    <#
    .SYNOPSIS
    检测命令是否可用
    #>
    Param([String]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Install-Protoc {
    <#
    .SYNOPSIS
    安装 protoc（protocol buffers 编译器）
    #>
    if (Test-Command 'protoc') {
        Write-Output "[OK] protoc 已安装: $(protoc --version)"
        return
    }
    Write-Output "[..] 正在安装 protoc ..."
    winget install Google.Protobuf --accept-source-agreements --accept-package-agreements
    if (-not $?) {
        Write-Error "protoc 安装失败，请手动安装后重试"
        exit 1
    }
    Write-Output "[OK] protoc 安装完成"
}

function Install-NodeDeps {
    <#
    .SYNOPSIS
    检查并安装 Node.js 和 corepack
    #>
    if (-not (Test-Command 'node')) {
        Write-Output "[..] 未检测到 Node.js，请先安装 Node.js 20.x"
        Write-Output "    下载地址: https://nodejs.org/"
        exit 1
    }
    $NodeVersion = (node --version)
    Write-Output "[OK] Node.js: $NodeVersion"

    # 启用 corepack 以支持 yarn 版本管理
    try { corepack enable 2>$null } catch { }
}

function Install-ISCC {
    <#
    .SYNOPSIS
    检查 Inno Setup (ISCC) 是否可用
    #>
    if (Test-Command 'ISCC') {
        Write-Output "[OK] Inno Setup (ISCC) 已安装"
        return
    }
    Write-Output "[..] 未检测到 ISCC，正在安装 Inno Setup ..."
    winget install JRSoftware.InnoSetup --accept-source-agreements --accept-package-agreements
    if (-not $?) {
        Write-Error "Inno Setup 安装失败，请手动安装后重试"
        Write-Output "    下载地址: https://jrsoftware.org/isdl.php"
        exit 1
    }
    # 将 ISCC 加入当前会话 PATH
    $IsccPath = 'C:\Program Files (x86)\Inno Setup 6'
    if (Test-Path $IsccPath) {
        $env:PATH = "$IsccPath;$env:PATH"
    }
    Write-Output "[OK] Inno Setup 安装完成"
}

# ── 初始化 ──────────────────────────────────────────────────

$WORKSPACE_ROOT = (Get-Location).Path
Write-Output ""
Write-Output "========================================"
Write-Output "  Zap Windows Release Build"
Write-Output "  Channel : $Channel"
Write-Output "  目录    : $WORKSPACE_ROOT"
Write-Output "========================================"
Write-Output ""

# 自动检测架构
if (-not $Arch) {
    if ($env:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
        $Arch = 'x64'
    } elseif ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
        $Arch = 'arm64'
    } else {
        Write-Error "不支持的处理器架构: $env:PROCESSOR_ARCHITECTURE"
        exit 1
    }
}
Write-Output "[i] 目标架构: $Arch"

# 生成 release tag（优先从 git tag 获取，否则生成基于日期的临时 tag）
$GitDescribe = ''
try { $GitDescribe = git describe --tags --abbrev=0 2>$null } catch { }
if ($LASTEXITCODE -eq 0 -and $GitDescribe) {
    $env:GIT_RELEASE_TAG = $GitDescribe.Trim()
} else {
    $DateTag = "v0.$(Get-Date -Format 'yyyy.MM.dd.HHmm')"
    $env:GIT_RELEASE_TAG = $DateTag
}
Write-Output "[i] Release Tag: $env:GIT_RELEASE_TAG"

# ── 依赖安装 ────────────────────────────────────────────────

if (-not $SkipDeps) {
    Write-Output ""
    Write-Output "--- 安装构建依赖 ---"

    # 检查 Rust
    if (Test-Command 'cargo') {
        Write-Output "[OK] Rust: $(rustc --version)"
    } else {
        Write-Output "[..] 正在安装 Rust 工具链 ..."
        if (Test-Command 'winget') {
            winget install Rustlang.Rustup --accept-source-agreements --accept-package-agreements
            if ($LASTEXITCODE -ne 0) {
                Write-Error "Rust 安装失败，请手动安装后重试"
                exit 1
            }
            # 刷新当前会话 PATH 以包含 cargo
            $CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
            if (Test-Path "$CargoHome\bin") {
                $env:PATH = "$CargoHome\bin;$env:PATH"
            }
        } else {
            Write-Error "未找到 winget，请手动安装 Rust: https://rustup.rs/"
            exit 1
        }
        if (-not (Test-Command 'cargo')) {
            Write-Error "Rust 安装后仍未检测到 cargo，请重启终端后重试"
            exit 1
        }
        Write-Output "[OK] Rust 安装完成: $(rustc --version)"
    }

    # 安装 cargo-binstall
    Write-Output "[..] 安装 cargo-binstall ..."
    if (-not (Test-Command 'cargo-binstall')) {
        cargo install cargo-binstall@1.14.3 --locked
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "cargo-binstall 安装失败，将回退到 cargo install"
        }
    }

    # 安装 diesel_cli（仅本地构建需要，CI 环境跳过）
    if ($env:GITHUB_ACTIONS -ne 'true') {
        Write-Output "[..] 安装 diesel_cli ..."
        if (Test-Command 'cargo-binstall') {
            cargo binstall --force -y diesel_cli
        } else {
            try { cargo install diesel_cli --locked 2>$null } catch { }
        }
    }

    # 安装内部 channel config（外部贡献者可能无权限，失败可忽略）
    Write-Output "[..] 安装 channel config ..."
    $ChannelConfigScript = "$WORKSPACE_ROOT\script\install_channel_config"
    if (Test-Path $ChannelConfigScript) {
        try { & $ChannelConfigScript 2>$null } catch {
            Write-Output "[i] 跳过内部 channel config 安装（无仓库访问权限）"
        }
    } else {
        Write-Output "[i] 跳过内部 channel config 安装（脚本不存在）"
    }

    # 安装 cargo-about（用于生成第三方许可证）
    Write-Output "[..] 安装 cargo-about ..."
    try { cargo install --locked cargo-about@0.8.4 2>$null } catch { }
    if (-not $?) {
        Write-Warning "cargo-about 安装失败，可能影响许可证生成"
    }

    # 安装 protoc
    Install-Protoc

    # 安装 Node.js 依赖
    Install-NodeDeps

    # 安装 Inno Setup（仅生成安装包时需要）
    if (-not $SkipInstaller) {
        Install-ISCC
    }

    Write-Output "[OK] 依赖安装完成"
} else {
    Write-Output "[i] 跳过依赖安装 (-SkipDeps)"
}

# ── 构建 ────────────────────────────────────────────────────

Write-Output ""
Write-Output "--- 开始构建 ---"

$BundleScript = "$WORKSPACE_ROOT\script\windows\bundle.ps1"
if (-not (Test-Path $BundleScript)) {
    Write-Error "找不到构建脚本: $BundleScript"
    exit 1
}

# 构建 oss channel release 版本
# -Channel oss      : 使用 oss 渠道配置
# -ARCH $Arch       : 指定目标架构
# -SkipBuildInstaller / -SkipBuildBinary : 分步构建（CI 模式）
# 直接调用 bundle.ps1 一步完成 binary + installer 构建

$BundleArgs = @{
    CHANNEL = $Channel
    ARCH    = $Arch
}
if ($SkipInstaller) {
    # 仅构建二进制
    $BundleArgs['SKIP_BUILD_INSTALLER'] = $true
}

Write-Output "[..] 执行: $BundleScript -Channel $Channel -ARCH $Arch$(if ($SkipInstaller) { ' -SkipBuildInstaller' })"
Write-Output ""

& $BundleScript @BundleArgs

if ($LASTEXITCODE -ne 0) {
    Write-Error "构建失败 (exit code: $LASTEXITCODE)"
    exit $LASTEXITCODE
}

# ── 输出结果 ────────────────────────────────────────────────

Write-Output ""
Write-Output "========================================"
Write-Output "  构建成功!"
Write-Output "  Channel     : $Channel"
Write-Output "  架构        : $Arch"
Write-Output "  Release Tag : $env:GIT_RELEASE_TAG"
Write-Output "========================================"

# 显示产物位置
if ($SkipInstaller) {
    # 仅二进制模式，显示 exe 路径
    if ($Arch -eq 'arm64') {
        $ProfileDir = "$WORKSPACE_ROOT\target\aarch64-pc-windows-msvc\rlto"
    } else {
        $ProfileDir = "$WORKSPACE_ROOT\target\x86_64-pc-windows-msvc\rlto"
    }
    $ExePath = "$ProfileDir\zap-oss.exe"
    if (Test-Path $ExePath) {
        Write-Output ""
        Write-Output "  二进制文件: $ExePath"
        $FileSize = (Get-Item $ExePath).Length / 1MB
        Write-Output "  文件大小  : $([math]::Round($FileSize, 2)) MB"
    }
} else {
    # 安装包模式，显示 installer 路径
    $InstallerDir = "$WORKSPACE_ROOT\script\windows\Output"
    if (Test-Path $InstallerDir) {
        $Installer = Get-ChildItem -Path $InstallerDir -Filter '*.exe' | Select-Object -First 1
        if ($Installer) {
            Write-Output ""
            Write-Output "  安装包    : $($Installer.FullName)"
            $FileSize = $Installer.Length / 1MB
            Write-Output "  文件大小  : $([math]::Round($FileSize, 2)) MB"
        }
    }
}

Write-Output ""
