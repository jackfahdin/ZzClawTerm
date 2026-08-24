#include "ZzX11ViewportPrivate.h"

#include "ZzX11Viewport.h"

#ifdef Q_OS_WIN
#include <qt_windows.h>

namespace {

/// EnumChildWindows 回调上下文：目标类名 + 目标矩形。
struct FollowEnumContext
{
    const wchar_t *className; ///< 待匹配的 Win32 类名（ZzXsrv/x）
    RECT rect;                ///< SetWindowPos 目标矩形（客户区坐标）
};

/// 匹配类名的子窗口直接 SetWindowPos 拉伸；无法匹配则跳过继续枚举。
BOOL CALLBACK followEnumProc(HWND hwnd, LPARAM lParam)
{
    auto *ctx = reinterpret_cast<FollowEnumContext *>(lParam);
    wchar_t name[64] = {};
    if (GetClassNameW(hwnd, name, 64) <= 0 || wcscmp(name, ctx->className) != 0)
        return TRUE;
    SetWindowPos(hwnd, nullptr,
                 ctx->rect.left, ctx->rect.top,
                 ctx->rect.right - ctx->rect.left,
                 ctx->rect.bottom - ctx->rect.top,
                 SWP_NOZORDER | SWP_NOACTIVATE);
    return TRUE;
}

} // namespace
#endif // Q_OS_WIN

void ZzX11ViewportPrivate::repositionXChildWindows(quintptr containerHwnd, const QRect &rect) const
{
#ifdef Q_OS_WIN
    const QString className = ZzX11Viewport::x11WindowClassName();
    FollowEnumContext ctx;
    ctx.className = reinterpret_cast<const wchar_t *>(className.utf16());
    ctx.rect = RECT{rect.x(), rect.y(), rect.x() + rect.width(), rect.y() + rect.height()};
    EnumChildWindows(reinterpret_cast<HWND>(containerHwnd), followEnumProc,
                     reinterpret_cast<LPARAM>(&ctx));
#else
    Q_UNUSED(containerHwnd);
    Q_UNUSED(rect);
#endif
}
