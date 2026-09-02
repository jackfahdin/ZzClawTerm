#pragma once

#include <functional>

#include <QtCore/QHash>
#include <QtCore/QList>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtWidgets/QWidget>

#include "ZzPanelInterface.h"
#include "ZzSftpOps.h"
#include "transport/ZzTransportInterface.h"

class QLineEdit;
class QLabel;
class QStandardItemModel;
class QToolButton;
class QTreeView;
class QTreeWidget;
class QTreeWidgetItem;
class ZzSshConnection;
class ZzSshTransportAdapter;
class ZzTabManager;
class ZzTerminalView;

/**
 * @brief SFTP 侧边栏面板：远程目录浏览、文件操作与传输队列（规格 §2.3 面板）。
 *
 * 经 ZzPanelRegistry 注册、ZzAppShell 装配为右侧侧栏面板。会话跟随
 * setTabManager() 注入的 ZzTabManager：绑定当前标签焦点窗格的 SSH 连接
 * （切标签/切窗格自动跟随）；本地 PTY 会话显示不可用提示。
 *
 * 网络隔离：面板只依赖 ZzSftpOps 抽象，生产路径由操作工厂包装
 * ZzSshConnection::createSftpSession()，测试经 setOpsFactory() 注入 mock。
 * 所有 SFTP 调用本身异步，面板不在 GUI 线程做任何等待；目录列举结果
 * 按批（kFillBatchSize）填充模型，避免大目录阻塞 UI。
 */
class ZzSftpPanel : public QWidget, public ZzPanelInterface
{
    Q_OBJECT
public:
    /**
     * @brief SFTP 操作工厂：由 SSH 连接创建操作门面。
     * @param connection 已连接的 SSH 连接。
     * @param parent 操作对象的 QObject 父对象（面板持有其生命周期）。
     */
    using ZzSftpOpsFactory =
        std::function<ZzSftpOps *(ZzSshConnection *connection, QObject *parent)>;

    explicit ZzSftpPanel(QWidget *parent = nullptr);

    // ---- ZzPanelInterface ----
    [[nodiscard]] QString panelId() const override;
    [[nodiscard]] QString panelTitle() const override;
    [[nodiscard]] QWidget *panelWidget() override;

    /** @brief 绑定标签管理器（ZzAppShell 装配时注入；可重复调用）。 */
    void setTabManager(ZzTabManager *tabs);

    /** @brief 替换操作工厂（测试注入 mock 用），对后续绑定生效。 */
    void setOpsFactory(ZzSftpOpsFactory factory);

    // ---- 测试观察口（等价于 UI 操作，离屏环境不模拟鼠标/对话框） ----
    /** @brief 导航到指定远端目录（等价路径栏回车/双击目录）。 */
    void navigateTo(const QString &path);
    /** @brief 刷新当前目录（等价刷新按钮）。 */
    void triggerRefresh();
    /** @brief 返回上级目录（等价上级按钮）。 */
    void triggerUp();
    /**
     * @brief 直接附着 SFTP 操作门面（测试注入 mock 用）。
     * @note 生产路径由 evaluateBinding 经操作工厂创建；注入对象不得
     *       以本面板为父（拆除时不接管其生命周期）。
     */
    void attachOpsForTesting(ZzSftpOps *ops);
    /** @brief 上传一批本地文件到当前目录（等价上传按钮→多选对话框确认）。 */
    void startUploads(const QStringList &localPaths);
    /** @brief 下载远端文件到本地路径（等价下载按钮→保存对话框确认）。 */
    void startDownload(const QString &remotePath, const QString &localPath);
    /** @brief 上传本地目录到当前远端目录（等价上传文件夹按钮→选目录确认）。 */
    void startUploadDir(const QString &localDir);
    /** @brief 下载远端目录到本地父目录下（等价目录右键→下载）。 */
    void startDownloadDir(const QString &remotePath, const QString &localParentDir);
    /** @brief 在当前目录新建子目录（等价新建目录按钮→输入确认）。 */
    void requestMakeDir(const QString &name);
    /** @brief 删除远端文件/空目录（等价删除按钮→确认）。 */
    void requestRemove(const QString &path, bool isDir);
    /** @brief 重命名远端条目（等价重命名按钮→输入确认）。 */
    void requestRename(const QString &path, const QString &newName);
    /** @brief 取消进行中的传输（等价传输行取消按钮）。 */
    void requestCancelTransfer(quint64 requestId);
    /** @brief 按名称选中目录列表中的一行（等价单击）。@return 找到并选中。 */
    bool selectEntry(const QString &name);

    /** @brief 当前远端目录路径（未连接为空串）。 */
    [[nodiscard]] QString currentPath() const;
    /** @brief 目录列表可见条目数（分批填充，未填满时小于总数）。 */
    [[nodiscard]] int visibleEntryCount() const;
    /** @brief 传输队列行数（含历史）。 */
    [[nodiscard]] int transferRowCount() const;
    /** @brief 指定传输的状态列文本（含换绑后历史行回退；未知请求返回空串）。 */
    [[nodiscard]] QString transferStatusText(quint64 requestId) const;
    /** @brief 底部状态/提示文本。 */
    [[nodiscard]] QString statusText() const;
    /** @brief 目录列举/填充是否进行中（加载卡死回归测试用）。 */
    [[nodiscard]] bool isLoading() const;
    /** @brief 是否已附着 SFTP 操作门面（等价面板处于可用会话上）。 */
    [[nodiscard]] bool opsAttached() const;

signals:
    /** @brief 需要状态栏展示的瞬时消息（ZzAppShell 接状态栏，规格 §八）。 */
    void statusMessage(const QString &message);

protected:
    /** @brief LanguageChange 时重设全部静态文本。 */
    void changeEvent(QEvent *event) override;

private slots:
    void onCurrentViewChanged(ZzTerminalView *view);
    void onOpened();
    void onOpsError(int code, const QString &message);
    void onClosed();
    void onDirListed(quint64 requestId, const QList<ZzSftpFileInfo> &entries);
    void onOperationFinished(quint64 requestId);
    void onOperationError(quint64 requestId, int code, const QString &message);
    void onTransferProgress(quint64 requestId, qint64 done, qint64 total);
    void onTransferFinished(quint64 requestId);
    void onTransferError(quint64 requestId, int code, const QString &message);

    void onUpClicked();
    void onPathEdited();
    void onEntryDoubleClicked(const QModelIndex &index);
    void onUploadClicked();
    void onUploadDirClicked();
    void onDownloadClicked();
    void onDownloadDirClicked();
    void onMakeDirClicked();
    void onDeleteClicked();
    void onRenameClicked();
    void showDirContextMenu(const QPoint &pos);

private:
    /** @brief 重设全部静态文本（构造与 LanguageChange 共用单一路径）。 */
    void retranslateUi();
    /** @brief 大目录分批填充：每批条目数（UI 线程一次填充上限）。 */
    static constexpr int kFillBatchSize = 500;
    /** @brief 传输队列已结束（非进行中）历史行上限，超出时移除最早的行。 */
    static constexpr int kMaxTransferHistory = 100;

    /** @brief 默认操作工厂：conn->createSftpSession() 包装为 ZzSftpSessionOps。 */
    static ZzSftpOps *createDefaultOps(ZzSshConnection *connection, QObject *parent);
    /** @brief 按当前绑定状态求值：换绑连接、创建/拆除操作门面、刷新提示。 */
    void evaluateBinding();
    /** @brief 无可用连接时的提示文案（无会话/本地会话/等待连接）。 */
    void updateUnavailableHint(ZzSshTransportAdapter *ssh);
    /** @brief 附着操作门面并接线（已打开的会话立即补发 onOpened 语义）。 */
    void attachOps(ZzSftpOps *ops);
    /** @brief 拆除当前操作门面（换绑/关闭时调用；进行中传输行标记中断）。 */
    void detachOps();
    /** @brief 清空目录模型与路径栏（换绑/断开时调用）。 */
    void clearListing();
    /** @brief 按 m_ops/加载状态刷新工具按钮可用性。 */
    void updateAvailability();
    void setStatus(const QString &text);
    /** @brief 从 m_fillQueue 取下一批填充模型；未填完则排队下一拍事件循环。
     *        延迟回调按 m_fillGeneration 代际校验，换绑/重新导航后的过期回调直接丢弃。 */
    void fillNextBatch();
    void appendEntryRow(const ZzSftpFileInfo &info);
    /** @brief 传输队列新增一行（进度条+取消按钮），并修剪超上限的历史行。 */
    void addTransferRow(quint64 requestId, const QString &name, const QString &direction);
    /** @brief 已结束传输行超过 kMaxTransferHistory 时移除最早的行（含哈希清理）。 */
    void pruneTransferHistory();
    /** @brief 当前选中条目的远端路径（无选中返回空串）。 */
    [[nodiscard]] QString selectedPath(bool *isDir = nullptr) const;

    QPointer<ZzTabManager> m_tabs;            ///< 非拥有（窗口页面持有）
    QPointer<ZzTerminalView> m_boundView;     ///< 当前绑定的焦点窗格
    QMetaObject::Connection m_viewStateConn;  ///< 绑定视图 stateChanged 的连接句柄
    ZzSshConnection *m_boundConn = nullptr;   ///< 当前绑定的 SSH 连接（非拥有）
    ZzSftpOps *m_ops = nullptr;               ///< 操作门面（工厂产物以面板为父）
    ZzSftpOpsFactory m_opsFactory;

    QLineEdit *m_pathEdit = nullptr;
    QTreeView *m_dirView = nullptr;
    QStandardItemModel *m_dirModel = nullptr;
    QTreeWidget *m_transferView = nullptr;
    QLabel *m_statusLabel = nullptr;
    QToolButton *m_upButton = nullptr;
    QToolButton *m_refreshButton = nullptr;
    QToolButton *m_uploadButton = nullptr;
    QToolButton *m_uploadDirButton = nullptr;
    QToolButton *m_downloadButton = nullptr;
    QToolButton *m_mkdirButton = nullptr;
    QToolButton *m_deleteButton = nullptr;
    QToolButton *m_renameButton = nullptr;

    QString m_currentPath;              ///< 当前远端目录
    QString m_pendingPath;              ///< 列举请求对应的目标目录
    quint64 m_listReqId = 0;            ///< 进行中的 listDir 请求 ID（0=无）
    quint64 m_fillGeneration = 0;       ///< 填充代际：导航/换绑时递增，作废过期延迟回调
    QList<ZzSftpFileInfo> m_fillQueue;  ///< 待分批填充的条目
    bool m_loading = false;             ///< 目录列举/填充进行中
    QSet<quint64> m_refreshRequests;    ///< 完成后需刷新目录的简单操作请求
    QSet<quint64> m_cancelled;          ///< 用户已请求取消的传输请求
    QHash<quint64, QTreeWidgetItem *> m_transferRows; ///< 请求 ID → 传输行
};
