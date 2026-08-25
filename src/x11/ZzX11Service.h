#pragma once

#include <QObject>
#include <QString>

#include "x11/ZzXAuthority.h"
#include "x11/ZzXServerManager.h" // ZzXLocalEndpoint

class ZzXServerDownloader;

/**
 * @brief 应用级共享 X server 门面（M5 规格 §4.1：对齐 MobaXterm 单例语义）。
 *
 * 全局单实例（ZzAppShell 持有）：全局开关开启时拉起 ZzXsrv（-multiwindow）供全部
 * 会话共享；会话只读查询 display/cookie/localEndpoint 并经 ensureRunning() 懒重拉，
 * 不拥有生命周期（关会话不杀，应用退出随析构终止）。全局开关关闭时
 * start()/ensureRunning() 为空操作。嵌入实验模式不经由本类（会话自带独立 server）。
 * cookie 在 server 就绪时生成一次，所有会话共用（xauth 0600 + 仅 127.0.0.1，
 * 威胁模型与现状同级，规格 §4.1）。
 */
class ZzX11Service : public QObject
{
    Q_OBJECT
public:
    explicit ZzX11Service(QObject *parent = nullptr);

    /** @brief 全局开关：开→立即拉起（幂等）；关→停止并阻止后续拉起。 */
    void setEnabled(bool enabled);
    /** @brief 当前开关状态。 */
    [[nodiscard]] bool isEnabled() const { return m_enabled; }

    /** @brief 拉起共享 server（幂等：已禁用/启动中/运行中均为空操作）。 */
    void start();
    /** @brief 停止共享 server（异步收尾，语义同 ZzXServerManager::stop）。 */
    void stop();
    /** @brief 会话侧懒重拉入口：等价 start()（规格 §4.3 lazy 重拉）。 */
    void ensureRunning() { start(); }

    /** @brief server 是否在运行。 */
    [[nodiscard]] bool isRunning() const;
    /** @brief 当前 display 号；未运行返回 -1。 */
    [[nodiscard]] int display() const { return m_display; }
    /** @brief 共享 MIT-MAGIC-COOKIE-1（hex）；未运行返回空串。 */
    [[nodiscard]] QString cookie() const { return m_cookie; }
    /** @brief 本地接入端点（透传 ZzXServerManager）。 */
    [[nodiscard]] ZzXLocalEndpoint localEndpoint() const;

    /** @brief 测试注入：以桩可执行替代真实 server 程序（透传 ZzXServerManager）。 */
    void setServerProgramForTesting(const QString &program);

signals:
    void serverStarted(int display);              ///< server 就绪（含懒重拉成功）
    void startFailed(const QString &message);     ///< 启动失败（下载失败/无 display/授权写入失败）
    void serverCrashed(const QString &message);   ///< 非预期退出（不自动热恢复）

private:
    void onDownloaderReady(const QString &executablePath); ///< 仅 Windows

    ZzXServerManager *m_manager = nullptr;        ///< 本对象为父
    ZzXServerDownloader *m_downloader = nullptr;  ///< 仅 Windows：本对象为父，惰性创建
    ZzXAuthority m_authority;                     ///< 值成员：无状态 cookie/xauth 工具
    QString m_programOverride;                    ///< 测试注入的桩程序路径（空=真实 server）
    QString m_cookie;                             ///< 共享 cookie（server 就绪时生成）
    int m_display = -1;                           ///< 当前 display 号
    bool m_enabled = false;                       ///< 全局开关（ZzAppShell 按设置驱动）
    bool m_starting = false;                      ///< 下载/拉起进行中（start 幂等判定）
};
