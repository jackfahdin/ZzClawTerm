#pragma once

#include <QObject>
#include <QString>

class QProcess;

/**
 * @brief X server 本地接入端点（供后续 SSH X11 转发桥接使用）。
 *
 * 与库侧 ZzSshX11Bridge::LocalEndpoint 同构的轻量结构：TCP 模式填写 host/port，
 * Unix 域套接字模式填写 localSocketPath。任务 6 传输接线时再做类型适配。
 */
struct ZzXLocalEndpoint
{
    QString host;            ///< TCP 模式下的监听地址（Windows 恒为 127.0.0.1）
    quint16 port = 0;        ///< TCP 模式下的端口（6000 + display 号）
    QString localSocketPath; ///< Unix 域套接字路径（/tmp/.X11-unix/X<display>），TCP 模式为空
};

/**
 * @brief X server 进程生命周期管理与 display 号分配。
 *
 * Windows：拉起 vcxsrv.exe（-multiwindow -clipboard -listen tcp -auth），区分
 * 崩溃与主动停止并支持重启；Unix：系统 X server 已由桌面环境运行，start() 只
 * 从 $DISPLAY 解析 display 号并记录端点，不拉起进程。
 */
class ZzXServerManager : public QObject
{
    Q_OBJECT
public:
    explicit ZzXServerManager(QObject *parent = nullptr);

    /**
     * @brief 分配空闲 display：从 0 递增探测 6000+N 端口可绑定。
     * @return 空闲 display 号；0~99 全被占用时返回 -1。
     */
    static int allocateDisplay();

    /**
     * @brief 启动 server（Windows：vcxsrv.exe；Unix 复用系统 X server 只记录端点）。
     * @param executablePath vcxsrv 可执行路径（Unix 无进程分支忽略）。
     * @param xauthorityPath 授权文件路径（拼入 -auth 参数）。
     * @param display 目标 display 号（Unix 无进程分支优先采用 $DISPLAY 解析值）。
     */
    void start(const QString &executablePath, const QString &xauthorityPath, int display);

    /** @brief 主动停止 server 进程并释放 display（Unix 无进程分支仅复位状态）。 */
    void stop();

    /** @brief server 当前是否在运行。 */
    bool isRunning() const;

    /** @brief 当前 display 号；未运行时返回 -1。 */
    int display() const;

    /**
     * @brief 崩溃后按上次 start() 参数重新拉起 server。
     * 从未 start 过或已在运行时为空操作。
     */
    void restart();

    /**
     * @brief 本地端点：TCP 模式 {127.0.0.1, 6000+display}；
     * Unix 复用系统 server 时 {/tmp/.X11-unix/X<display>}。
     */
    ZzXLocalEndpoint localEndpoint() const;

    /** @brief 测试注入点：以桩可执行替代真实 server 程序（空串恢复默认）。 */
    void setServerProgramForTesting(const QString &program);

signals:
    void started(int display);          ///< server 就绪（Windows 进程拉起成功；Unix 端点已记录）
    void crashed(const QString &message); ///< 非预期退出（消息含退出码），区别于主动 stop()
    void stopped();                     ///< 主动停止或非零退出的干净退出后复位

private:
    /** @brief 以给定参数拉起 server 进程（start 与 restart 共用）。 */
    void launchProcess(const QString &program, const QString &xauthorityPath, int displayNum);

    /** @brief 从 $DISPLAY 解析 display 号（":0.0"/"localhost:10.0"），失败返回 -1。 */
    static int parseSystemDisplay();

    QProcess *m_process = nullptr; ///< 持有的 server 进程（Unix 无进程分支恒为空）
    int m_display = -1;            ///< 当前 display 号
    bool m_running = false;        ///< 运行标志
    bool m_stopping = false;       ///< 主动停止标志：区分 crashed 与 stopped
    QString m_programOverride;     ///< 测试注入的桩程序路径
    QString m_lastProgram;         ///< 上次启动程序（供 restart）
    QString m_lastXauthorityPath;  ///< 上次授权文件路径（供 restart）
    int m_lastDisplay = -1;        ///< 上次 display 号（供 restart）
};
