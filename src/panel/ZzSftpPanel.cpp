#include "ZzSftpPanel.h"

#include <algorithm>

#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QLocale>
#include <QtCore/QTimer>
#include <QtGui/QStandardItemModel>
#include <QtWidgets/QFileDialog>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QInputDialog>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QMenu>
#include <QtWidgets/QMessageBox>
#include <QtWidgets/QProgressBar>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QTreeWidget>
#include <QtWidgets/QVBoxLayout>

#include <ZzSshConnection.h>

#include "ZzSftpSessionOps.h"
#include "settings/ZzAppSettings.h"
#include "tab/ZzTabManager.h"
#include "terminal/ZzTerminalView.h"
#include "transport/ZzSshTransport.h"

namespace {

/** @brief 目录模型角色键：条目完整远端路径（列 0）。 */
constexpr int kPathRole = Qt::UserRole + 1;
/** @brief 目录模型角色键：条目是否目录（列 0）。 */
constexpr int kIsDirRole = Qt::UserRole + 2;

/** @brief 传输队列列序。 */
enum TransferColumn {
    kColName = 0,   ///< 文件名
    kColDirection,  ///< 方向（上传/下载）
    kColProgress,   ///< 进度条（setItemWidget）
    kColStatus,     ///< 状态文本
    kColAction      ///< 取消按钮（setItemWidget，结束后移除）
};

/** @brief 拼接远端路径（dir 以 / 结尾时不重复分隔符）。 */
QString zzJoinPath(const QString &dir, const QString &name)
{
    return dir.endsWith(QLatin1Char('/')) ? dir + name
                                          : dir + QLatin1Char('/') + name;
}

/** @brief 远端路径的上级目录（根目录的上级仍是根）。 */
QString zzParentPath(const QString &path)
{
    if (path.isEmpty() || path == QLatin1String("/")) {
        return QStringLiteral("/");
    }
    const int idx = path.lastIndexOf(QLatin1Char('/'));
    return idx <= 0 ? QStringLiteral("/") : path.left(idx);
}

/** @brief 权限位 → ls 风格字符串（如 drwxr-xr-x）。 */
QString zzFormatPermissions(quint32 permissions)
{
    QString result;
    result.reserve(10);
    if (LIBSSH2_SFTP_S_ISDIR(permissions)) {
        result += QLatin1Char('d');
    } else if (LIBSSH2_SFTP_S_ISLNK(permissions)) {
        result += QLatin1Char('l');
    } else {
        result += QLatin1Char('-');
    }
    const struct { quint32 bit; QChar ch; } kBits[] = {
        { 0400, QLatin1Char('r') }, { 0200, QLatin1Char('w') },
        { 0100, QLatin1Char('x') }, { 0040, QLatin1Char('r') },
        { 0020, QLatin1Char('w') }, { 0010, QLatin1Char('x') },
        { 0004, QLatin1Char('r') }, { 0002, QLatin1Char('w') },
        { 0001, QLatin1Char('x') },
    };
    for (const auto &bit : kBits) {
        result += (permissions & bit.bit) ? bit.ch : QLatin1Char('-');
    }
    return result;
}

/** @brief 目录列举排序：目录在前，同类按名称（大小写不敏感）。 */
bool zzEntryLess(const ZzSftpFileInfo &a, const ZzSftpFileInfo &b)
{
    if (a.isDir() != b.isDir()) {
        return a.isDir();
    }
    return QString::compare(a.name, b.name, Qt::CaseInsensitive) < 0;
}

} // namespace

ZzSftpPanel::ZzSftpPanel(QWidget *parent)
    : QDockWidget(QStringLiteral("SFTP"), parent)
{
    setObjectName(panelId()); // QDockWidget 布局持久化键
    setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea); // 可停靠左右
    setFeatures(QDockWidget::DockWidgetClosable
                | QDockWidget::DockWidgetMovable); // 可折叠/拖动

    auto *central = new QWidget(this);
    auto *layout = new QVBoxLayout(central);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    // 工具行：上级/刷新/上传/下载/新建目录/删除/重命名
    auto *toolbar = new QHBoxLayout;
    toolbar->setSpacing(2);
    const auto addButton = [this, toolbar](const QString &text,
                                           const QString &tip,
                                           void (ZzSftpPanel::*slot)()) {
        auto *button = new QToolButton(this);
        button->setText(text);
        button->setToolTip(tip);
        toolbar->addWidget(button);
        connect(button, &QToolButton::clicked, this, slot);
        return button;
    };
    m_upButton = addButton(QStringLiteral("上级"), QStringLiteral("返回上级目录"),
                           &ZzSftpPanel::onUpClicked);
    m_refreshButton = addButton(QStringLiteral("刷新"), QStringLiteral("刷新当前目录"),
                                &ZzSftpPanel::triggerRefresh);
    toolbar->addSpacing(8);
    m_uploadButton = addButton(QStringLiteral("上传"), QStringLiteral("上传本地文件（可多选）"),
                               &ZzSftpPanel::onUploadClicked);
    m_downloadButton = addButton(QStringLiteral("下载"), QStringLiteral("下载选中文件"),
                                 &ZzSftpPanel::onDownloadClicked);
    m_mkdirButton = addButton(QStringLiteral("新建目录"), QStringLiteral("在当前目录新建子目录"),
                              &ZzSftpPanel::onMakeDirClicked);
    m_deleteButton = addButton(QStringLiteral("删除"), QStringLiteral("删除选中文件/空目录"),
                               &ZzSftpPanel::onDeleteClicked);
    m_renameButton = addButton(QStringLiteral("重命名"), QStringLiteral("重命名选中条目"),
                               &ZzSftpPanel::onRenameClicked);
    toolbar->addStretch(1);
    layout->addLayout(toolbar);

    // 路径栏：回车导航
    m_pathEdit = new QLineEdit(this);
    m_pathEdit->setPlaceholderText(QStringLiteral("远端路径，回车跳转"));
    connect(m_pathEdit, &QLineEdit::returnPressed,
            this, &ZzSftpPanel::onPathEdited);
    layout->addWidget(m_pathEdit);

    // 目录列表：名称/大小/权限/修改时间
    m_dirModel = new QStandardItemModel(this);
    m_dirModel->setHorizontalHeaderLabels({QStringLiteral("名称"),
                                           QStringLiteral("大小"),
                                           QStringLiteral("权限"),
                                           QStringLiteral("修改时间")});
    m_dirView = new QTreeView(this);
    m_dirView->setModel(m_dirModel);
    m_dirView->setRootIsDecorated(false);
    m_dirView->setUniformRowHeights(true); // 大目录滚动性能
    m_dirView->setSelectionMode(QAbstractItemView::SingleSelection);
    m_dirView->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_dirView->setContextMenuPolicy(Qt::CustomContextMenu);
    m_dirView->setColumnWidth(0, 180);
    connect(m_dirView, &QTreeView::doubleClicked,
            this, &ZzSftpPanel::onEntryDoubleClicked);
    connect(m_dirView, &QTreeView::customContextMenuRequested,
            this, &ZzSftpPanel::showDirContextMenu);
    connect(m_dirView->selectionModel(), &QItemSelectionModel::selectionChanged,
            this, [this] { updateAvailability(); });
    layout->addWidget(m_dirView, 1);

    // 传输队列：文件/方向/进度/状态/取消
    m_transferView = new QTreeWidget(this);
    m_transferView->setHeaderLabels({QStringLiteral("文件"), QStringLiteral("方向"),
                                     QStringLiteral("进度"), QStringLiteral("状态"),
                                     QStringLiteral("操作")});
    m_transferView->setRootIsDecorated(false);
    m_transferView->setMaximumHeight(140);
    m_transferView->setColumnWidth(0, 140);
    layout->addWidget(m_transferView);

    m_statusLabel = new QLabel(this);
    m_statusLabel->setWordWrap(true);
    layout->addWidget(m_statusLabel);

    setWidget(central);
    setStatus(QStringLiteral("无活动会话"));
    updateAvailability();

    // M6：设置变更兜底重应用传输块大小（0=自动同样传递，恢复 BDP 自适应）；
    // 会话附着点（attachOps）已对新会话应用，这里覆盖已打开会话
    connect(&ZzAppSettings::instance(), &ZzAppSettings::settingsChanged,
            this, [this] {
        if (m_ops) {
            m_ops->setTransferBlockSize(ZzAppSettings::instance().sftpBlockSize());
        }
    });
}

QString ZzSftpPanel::panelId() const
{
    return QStringLiteral("sftp");
}

QString ZzSftpPanel::panelTitle() const
{
    return QStringLiteral("SFTP");
}

QWidget *ZzSftpPanel::panelWidget()
{
    return this;
}

void ZzSftpPanel::setTabManager(ZzTabManager *tabs)
{
    if (m_tabs) {
        disconnect(m_tabs, &ZzTabManager::currentViewChanged,
                   this, &ZzSftpPanel::onCurrentViewChanged);
    }
    m_tabs = tabs;
    if (tabs) {
        connect(tabs, &ZzTabManager::currentViewChanged,
                this, &ZzSftpPanel::onCurrentViewChanged);
        onCurrentViewChanged(tabs->viewAt(tabs->currentIndex())); // 初始绑定
    } else {
        onCurrentViewChanged(nullptr);
    }
}

void ZzSftpPanel::setOpsFactory(ZzSftpOpsFactory factory)
{
    m_opsFactory = std::move(factory);
}

ZzSftpOps *ZzSftpPanel::createDefaultOps(ZzSshConnection *connection, QObject *parent)
{
    return new ZzSftpSessionOps(
        connection ? connection->createSftpSession() : nullptr, parent);
}

void ZzSftpPanel::onCurrentViewChanged(ZzTerminalView *view)
{
    if (m_boundView) {
        disconnect(m_viewStateConn);
        m_viewStateConn = {};
    }
    m_boundView = view;
    if (view) {
        m_viewStateConn = connect(view, &ZzTerminalView::stateChanged, this,
                [this](ZzTransportInterface::State) { evaluateBinding(); });
    }
    evaluateBinding();
}

void ZzSftpPanel::evaluateBinding()
{
    // 取焦点窗格的 SSH 连接（仅 Connected 状态可创建 SFTP 会话）
    ZzSshConnection *conn = nullptr;
    ZzSshTransportAdapter *ssh = nullptr;
    if (m_boundView) {
        ssh = qobject_cast<ZzSshTransportAdapter *>(m_boundView->transport());
        if (ssh && ssh->sshConnection()
            && ssh->sshConnection()->state() == ZzSshConnection::State::Connected) {
            conn = ssh->sshConnection();
        }
    }
    if (conn == m_boundConn) {
        if (!conn) {
            updateUnavailableHint(ssh); // 同为无连接也要刷新提示（视图可能已换）
        }
        updateAvailability();
        return;
    }
    detachOps();
    m_boundConn = conn;
    if (conn) {
        setStatus(QStringLiteral("正在打开 SFTP 会话…"));
        attachOps(m_opsFactory ? m_opsFactory(conn, this)
                               : createDefaultOps(conn, this));
    } else {
        clearListing();
        updateUnavailableHint(ssh);
        updateAvailability();
    }
}

void ZzSftpPanel::updateUnavailableHint(ZzSshTransportAdapter *ssh)
{
    if (!m_boundView) {
        setStatus(QStringLiteral("无活动会话"));
    } else if (!ssh) {
        setStatus(QStringLiteral("当前会话为本地 Shell，SFTP 不可用"));
    } else {
        setStatus(QStringLiteral("等待 SSH 连接…"));
    }
}

void ZzSftpPanel::attachOps(ZzSftpOps *ops)
{
    m_ops = ops;
    // M6：新会话附着即应用当前传输块大小设置（0=自动 BDP 自适应）
    ops->setTransferBlockSize(ZzAppSettings::instance().sftpBlockSize());
    connect(ops, &ZzSftpOps::opened, this, &ZzSftpPanel::onOpened);
    connect(ops, &ZzSftpOps::errorOccurred, this, &ZzSftpPanel::onOpsError);
    connect(ops, &ZzSftpOps::closed, this, &ZzSftpPanel::onClosed);
    connect(ops, &ZzSftpOps::dirListed, this, &ZzSftpPanel::onDirListed);
    connect(ops, &ZzSftpOps::operationFinished,
            this, &ZzSftpPanel::onOperationFinished);
    connect(ops, &ZzSftpOps::operationError,
            this, &ZzSftpPanel::onOperationError);
    connect(ops, &ZzSftpOps::transferProgress,
            this, &ZzSftpPanel::onTransferProgress);
    connect(ops, &ZzSftpOps::transferFinished,
            this, &ZzSftpPanel::onTransferFinished);
    connect(ops, &ZzSftpOps::transferError,
            this, &ZzSftpPanel::onTransferError);
    if (ops->isOpen()) {
        onOpened(); // 会话在附着前已打开（测试注入场景）
    }
    updateAvailability();
}

void ZzSftpPanel::detachOps()
{
    if (!m_ops) {
        return;
    }
    ZzSftpOps *ops = m_ops;
    m_ops = nullptr;
    disconnect(ops, nullptr, this, nullptr);
    ops->closeSession(); // 幂等；未打开为空操作
    if (ops->parent() == this) {
        ops->deleteLater(); // 工厂产物由面板回收；外部注入的 mock 不接管
    }
    m_listReqId = 0;
    m_fillQueue.clear();
    ++m_fillGeneration; // 作废未跑完的延迟填充回调
    m_refreshRequests.clear();
    m_cancelled.clear();
    m_loading = false;
    // 进行中的传输行标记中断（旧会话的请求 ID 不再配对）
    for (auto it = m_transferRows.begin(); it != m_transferRows.end(); ++it) {
        QTreeWidgetItem *item = it.value();
        if (item->text(kColStatus) == QStringLiteral("进行中")) {
            item->setText(kColStatus, QStringLiteral("已中断"));
            m_transferView->removeItemWidget(item, kColAction);
        }
    }
    m_transferRows.clear();
}

void ZzSftpPanel::onOpened()
{
    navigateTo(QStringLiteral("/"));
}

void ZzSftpPanel::onOpsError(int code, const QString &message)
{
    Q_UNUSED(code);
    setStatus(QStringLiteral("SFTP 会话错误：%1").arg(message));
    emit statusMessage(message);
}

void ZzSftpPanel::onClosed()
{
    if (sender() != m_ops) {
        return;
    }
    detachOps();
    m_boundConn = nullptr; // 会话已死：即使连接仍在也不再复用旧绑定
    setStatus(QStringLiteral("SFTP 会话已关闭"));
    updateAvailability();
    // 延迟重评估绑定：连接仍在（瞬时失败）时自动重建 SFTP 会话，
    // 连接已走时刷新为对应提示，避免面板停在"已关闭"直到切窗格
    QTimer::singleShot(500, this, [this] {
        if (!m_ops) {
            evaluateBinding();
        }
    });
}

// ---- 目录浏览 ----

void ZzSftpPanel::navigateTo(const QString &path)
{
    if (!m_ops || !m_ops->isOpen()) {
        setStatus(QStringLiteral("SFTP 会话未打开"));
        return;
    }
    const quint64 reqId = m_ops->listDir(path);
    if (reqId == 0) {
        setStatus(QStringLiteral("SFTP 会话未打开"));
        return;
    }
    m_listReqId = reqId;
    m_pendingPath = path;
    m_loading = true;
    ++m_fillGeneration; // 作废上一批未跑完的延迟填充回调
    m_fillQueue.clear();
    m_dirModel->removeRows(0, m_dirModel->rowCount());
    setStatus(QStringLiteral("加载 %1 …").arg(path));
    updateAvailability();
}

void ZzSftpPanel::triggerRefresh()
{
    if (!m_currentPath.isEmpty()) {
        navigateTo(m_currentPath);
    }
}

void ZzSftpPanel::triggerUp()
{
    onUpClicked();
}

void ZzSftpPanel::attachOpsForTesting(ZzSftpOps *ops)
{
    if (ops) {
        attachOps(ops);
    }
}

void ZzSftpPanel::onDirListed(quint64 requestId,
                              const QList<ZzSftpFileInfo> &entries)
{
    if (requestId != m_listReqId) {
        return; // 过期请求（连发导航时旧响应）
    }
    m_listReqId = 0;
    m_currentPath = m_pendingPath;
    m_pathEdit->setText(m_currentPath);

    QList<ZzSftpFileInfo> sorted;
    sorted.reserve(entries.size());
    for (const ZzSftpFileInfo &info : entries) {
        if (info.name == QLatin1String(".") || info.name == QLatin1String("..")) {
            continue; // 导航走"上级"按钮，不展示占位项
        }
        sorted.append(info);
    }
    std::sort(sorted.begin(), sorted.end(), zzEntryLess);
    m_fillQueue = sorted;
    fillNextBatch();
}

void ZzSftpPanel::fillNextBatch()
{
    int filled = 0;
    while (!m_fillQueue.isEmpty() && filled < kFillBatchSize) {
        appendEntryRow(m_fillQueue.takeFirst());
        ++filled;
    }
    if (!m_fillQueue.isEmpty()) {
        // 大目录分批填充：让出事件循环，避免阻塞 UI；延迟回调按代际校验，
        // 换绑/重新导航后过期回调直接丢弃（否则会提前结束新一轮加载状态）
        const quint64 generation = m_fillGeneration;
        QTimer::singleShot(0, this, [this, generation] {
            if (generation == m_fillGeneration) {
                fillNextBatch();
            }
        });
        return;
    }
    m_loading = false;
    setStatus(QStringLiteral("%1 项").arg(m_dirModel->rowCount()));
    updateAvailability();
}

void ZzSftpPanel::appendEntryRow(const ZzSftpFileInfo &info)
{
    const QString fullPath = zzJoinPath(m_pendingPath, info.name);
    auto *nameItem = new QStandardItem(info.name);
    nameItem->setEditable(false);
    nameItem->setData(fullPath, kPathRole);
    nameItem->setData(info.isDir(), kIsDirRole);

    auto *sizeItem = new QStandardItem(
        info.isDir() || info.size < 0
            ? QStringLiteral("-")
            : QLocale().formattedDataSize(info.size));
    sizeItem->setEditable(false);
    sizeItem->setTextAlignment(Qt::AlignRight | Qt::AlignVCenter);

    auto *permItem = new QStandardItem(
        info.permissions == 0 ? QStringLiteral("-")
                              : zzFormatPermissions(info.permissions));
    permItem->setEditable(false);

    auto *mtimeItem = new QStandardItem(
        info.mtime <= 0 ? QStringLiteral("-")
                        : QDateTime::fromSecsSinceEpoch(info.mtime)
                              .toLocalTime()
                              .toString(QStringLiteral("yyyy-MM-dd HH:mm")));
    mtimeItem->setEditable(false);

    m_dirModel->appendRow({nameItem, sizeItem, permItem, mtimeItem});
}

void ZzSftpPanel::clearListing()
{
    m_fillQueue.clear();
    m_dirModel->removeRows(0, m_dirModel->rowCount());
    m_currentPath.clear();
    m_pendingPath.clear();
    m_pathEdit->clear();
    m_listReqId = 0;
    m_loading = false;
}

void ZzSftpPanel::onUpClicked()
{
    navigateTo(zzParentPath(m_currentPath));
}

void ZzSftpPanel::onPathEdited()
{
    const QString path = m_pathEdit->text().trimmed();
    if (!path.isEmpty()) {
        navigateTo(path);
    }
}

void ZzSftpPanel::onEntryDoubleClicked(const QModelIndex &index)
{
    const QModelIndex first = index.siblingAtColumn(0);
    if (first.data(kIsDirRole).toBool()) {
        navigateTo(first.data(kPathRole).toString());
    }
}

// ---- 文件操作 ----

void ZzSftpPanel::startUploads(const QStringList &localPaths)
{
    if (!m_ops || !m_ops->isOpen() || m_currentPath.isEmpty()) {
        return;
    }
    for (const QString &localPath : localPaths) {
        const QString name = QFileInfo(localPath).fileName();
        if (name.isEmpty()) {
            continue;
        }
        const quint64 reqId =
            m_ops->upload(localPath, zzJoinPath(m_currentPath, name));
        if (reqId > 0) {
            addTransferRow(reqId, name, QStringLiteral("上传"));
        }
    }
}

void ZzSftpPanel::startDownload(const QString &remotePath, const QString &localPath)
{
    if (!m_ops || !m_ops->isOpen() || remotePath.isEmpty() || localPath.isEmpty()) {
        return;
    }
    const quint64 reqId = m_ops->download(remotePath, localPath);
    if (reqId > 0) {
        addTransferRow(reqId, remotePath.section(QLatin1Char('/'), -1),
                     QStringLiteral("下载"));
    }
}

void ZzSftpPanel::requestMakeDir(const QString &name)
{
    if (!m_ops || !m_ops->isOpen() || name.isEmpty()
        || name.contains(QLatin1Char('/')) || m_currentPath.isEmpty()) {
        return;
    }
    const quint64 reqId = m_ops->makeDir(zzJoinPath(m_currentPath, name));
    if (reqId > 0) {
        m_refreshRequests.insert(reqId);
    }
}

void ZzSftpPanel::requestRemove(const QString &path, bool isDir)
{
    if (!m_ops || !m_ops->isOpen() || path.isEmpty()) {
        return;
    }
    const quint64 reqId =
        isDir ? m_ops->removeDir(path) : m_ops->removeFile(path);
    if (reqId > 0) {
        m_refreshRequests.insert(reqId);
    }
}

void ZzSftpPanel::requestRename(const QString &path, const QString &newName)
{
    if (!m_ops || !m_ops->isOpen() || path.isEmpty() || newName.isEmpty()
        || newName.contains(QLatin1Char('/'))) {
        return;
    }
    const quint64 reqId =
        m_ops->rename(path, zzJoinPath(zzParentPath(path), newName));
    if (reqId > 0) {
        m_refreshRequests.insert(reqId);
    }
}

void ZzSftpPanel::onOperationFinished(quint64 requestId)
{
    if (m_refreshRequests.remove(requestId)) {
        triggerRefresh();
    }
}

void ZzSftpPanel::onOperationError(quint64 requestId, int code,
                                   const QString &message)
{
    Q_UNUSED(code);
    m_refreshRequests.remove(requestId);
    if (requestId == m_listReqId) {
        // listDir 失败：解除加载态，否则工具栏永久禁用（m_loading 卡在 true）
        m_listReqId = 0;
        m_loading = false;
        updateAvailability();
    }
    setStatus(QStringLiteral("操作失败：%1").arg(message));
    emit statusMessage(message);
}

// ---- 传输队列 ----

void ZzSftpPanel::addTransferRow(quint64 requestId, const QString &name,
                                 const QString &direction)
{
    auto *item = new QTreeWidgetItem(m_transferView);
    item->setText(kColName, name);
    item->setText(kColDirection, direction);
    item->setText(kColStatus, QStringLiteral("进行中"));
    item->setData(0, Qt::UserRole, QVariant::fromValue(requestId)); // 历史行回查用

    auto *bar = new QProgressBar(m_transferView);
    bar->setRange(0, 100);
    bar->setValue(0);
    m_transferView->setItemWidget(item, kColProgress, bar);

    auto *cancel = new QToolButton(m_transferView);
    cancel->setText(QStringLiteral("取消"));
    connect(cancel, &QToolButton::clicked, this,
            [this, requestId] { requestCancelTransfer(requestId); });
    m_transferView->setItemWidget(item, kColAction, cancel);

    m_transferRows.insert(requestId, item);
    pruneTransferHistory();
    m_transferView->scrollToBottom();
}

void ZzSftpPanel::pruneTransferHistory()
{
    // 只修剪已结束（非进行中）行：进行中的传输永不移除
    int finished = 0;
    for (int i = 0; i < m_transferView->topLevelItemCount(); ++i) {
        if (m_transferView->topLevelItem(i)->text(kColStatus)
            != QStringLiteral("进行中")) {
            ++finished;
        }
    }
    // 从最早的行开始移除（条目删除时其 itemWidget 随索引部件一并释放）
    for (int i = 0;
         finished > kMaxTransferHistory && i < m_transferView->topLevelItemCount();) {
        QTreeWidgetItem *item = m_transferView->topLevelItem(i);
        if (item->text(kColStatus) == QStringLiteral("进行中")) {
            ++i;
            continue;
        }
        m_transferRows.remove(item->data(0, Qt::UserRole).toULongLong()); // 防悬挂
        delete m_transferView->takeTopLevelItem(i);
        --finished;
    }
}

void ZzSftpPanel::requestCancelTransfer(quint64 requestId)
{
    if (!m_transferRows.contains(requestId)) {
        return;
    }
    m_cancelled.insert(requestId);
    if (m_ops) {
        m_ops->cancelTransfer(requestId);
    }
    // 会话已走时取消不会产生结局信号，直接标记
    if (!m_ops || !m_ops->isOpen()) {
        QTreeWidgetItem *item = m_transferRows.value(requestId);
        item->setText(kColStatus, QStringLiteral("已取消"));
        m_transferView->removeItemWidget(item, kColAction);
    }
}

void ZzSftpPanel::onTransferProgress(quint64 requestId, qint64 done, qint64 total)
{
    QTreeWidgetItem *item = m_transferRows.value(requestId);
    if (!item) {
        return;
    }
    // 进度信号已由库侧节流（≥1MB 或 ≥100ms），直接落进度条，不再额外刷新
    auto *bar = qobject_cast<QProgressBar *>(
        m_transferView->itemWidget(item, kColProgress));
    if (!bar) {
        return;
    }
    if (total > 0) {
        if (bar->maximum() == 0) {
            bar->setRange(0, 100); // 从忙碌态恢复为百分比
        }
        bar->setValue(static_cast<int>(done * 100 / total));
    } else {
        bar->setRange(0, 0); // 总量未知：忙碌指示
    }
}

void ZzSftpPanel::onTransferFinished(quint64 requestId)
{
    QTreeWidgetItem *item = m_transferRows.value(requestId);
    if (!item) {
        return;
    }
    item->setText(kColStatus, QStringLiteral("完成"));
    if (auto *bar = qobject_cast<QProgressBar *>(
            m_transferView->itemWidget(item, kColProgress))) {
        bar->setRange(0, 100);
        bar->setValue(100);
    }
    m_transferView->removeItemWidget(item, kColAction);
    m_cancelled.remove(requestId);
    pruneTransferHistory();
}

void ZzSftpPanel::onTransferError(quint64 requestId, int code,
                                  const QString &message)
{
    Q_UNUSED(code);
    QTreeWidgetItem *item = m_transferRows.value(requestId);
    if (!item) {
        return;
    }
    if (m_cancelled.remove(requestId)) {
        item->setText(kColStatus, QStringLiteral("已取消"));
    } else {
        item->setText(kColStatus, QStringLiteral("失败：%1").arg(message));
        emit statusMessage(message);
    }
    m_transferView->removeItemWidget(item, kColAction);
    pruneTransferHistory();
}

// ---- 工具按钮与右键菜单（对话框包装，测试走 request*/start* 观察口） ----

QString ZzSftpPanel::selectedPath(bool *isDir) const
{
    const QModelIndex index =
        m_dirView->currentIndex().siblingAtColumn(0);
    if (!index.isValid()) {
        return QString();
    }
    if (isDir) {
        *isDir = index.data(kIsDirRole).toBool();
    }
    return index.data(kPathRole).toString();
}

bool ZzSftpPanel::selectEntry(const QString &name)
{
    for (int row = 0; row < m_dirModel->rowCount(); ++row) {
        if (m_dirModel->item(row, 0)->text() == name) {
            m_dirView->setCurrentIndex(m_dirModel->index(row, 0));
            updateAvailability();
            return true;
        }
    }
    return false;
}

void ZzSftpPanel::onUploadClicked()
{
    startUploads(QFileDialog::getOpenFileNames(
        this, QStringLiteral("上传文件")));
}

void ZzSftpPanel::onDownloadClicked()
{
    bool isDir = false;
    const QString path = selectedPath(&isDir);
    if (path.isEmpty() || isDir) {
        emit statusMessage(QStringLiteral("请选择要下载的文件"));
        return;
    }
    const QString localPath = QFileDialog::getSaveFileName(
        this, QStringLiteral("下载到"),
        QDir::home().filePath(path.section(QLatin1Char('/'), -1)));
    if (!localPath.isEmpty()) {
        startDownload(path, localPath);
    }
}

void ZzSftpPanel::onMakeDirClicked()
{
    bool ok = false;
    const QString name = QInputDialog::getText(
        this, QStringLiteral("新建目录"), QStringLiteral("目录名："),
        QLineEdit::Normal, QString(), &ok);
    if (ok && !name.trimmed().isEmpty()) {
        requestMakeDir(name.trimmed());
    }
}

void ZzSftpPanel::onDeleteClicked()
{
    bool isDir = false;
    const QString path = selectedPath(&isDir);
    if (path.isEmpty()) {
        return;
    }
    const auto choice = QMessageBox::question(
        this, QStringLiteral("删除"),
        QStringLiteral("确定删除 %1 吗？").arg(path));
    if (choice == QMessageBox::Yes) {
        requestRemove(path, isDir);
    }
}

void ZzSftpPanel::onRenameClicked()
{
    const QString path = selectedPath();
    if (path.isEmpty()) {
        return;
    }
    bool ok = false;
    const QString name = QInputDialog::getText(
        this, QStringLiteral("重命名"), QStringLiteral("新名称："),
        QLineEdit::Normal, path.section(QLatin1Char('/'), -1), &ok);
    if (ok && !name.trimmed().isEmpty()) {
        requestRename(path, name.trimmed());
    }
}

void ZzSftpPanel::showDirContextMenu(const QPoint &pos)
{
    const QModelIndex index = m_dirView->indexAt(pos).siblingAtColumn(0);
    const bool ready = m_ops && m_ops->isOpen();
    QMenu menu(this);
    QAction *downloadAction = nullptr;
    QAction *renameAction = nullptr;
    QAction *deleteAction = nullptr;
    if (index.isValid()) {
        const bool isDir = index.data(kIsDirRole).toBool();
        downloadAction = menu.addAction(QStringLiteral("下载"));
        downloadAction->setEnabled(ready && !isDir);
        renameAction = menu.addAction(QStringLiteral("重命名"));
        renameAction->setEnabled(ready);
        deleteAction = menu.addAction(QStringLiteral("删除"));
        deleteAction->setEnabled(ready);
        menu.addSeparator();
    }
    QAction *uploadAction = menu.addAction(QStringLiteral("上传"));
    uploadAction->setEnabled(ready);
    QAction *mkdirAction = menu.addAction(QStringLiteral("新建目录"));
    mkdirAction->setEnabled(ready);
    QAction *refreshAction = menu.addAction(QStringLiteral("刷新"));
    refreshAction->setEnabled(ready);

    QAction *chosen = menu.exec(m_dirView->viewport()->mapToGlobal(pos));
    if (chosen == downloadAction && downloadAction) {
        onDownloadClicked();
    } else if (chosen == renameAction && renameAction) {
        onRenameClicked();
    } else if (chosen == deleteAction && deleteAction) {
        onDeleteClicked();
    } else if (chosen == uploadAction) {
        onUploadClicked();
    } else if (chosen == mkdirAction) {
        onMakeDirClicked();
    } else if (chosen == refreshAction) {
        triggerRefresh();
    }
}

// ---- 状态与观察口 ----

void ZzSftpPanel::setStatus(const QString &text)
{
    m_statusLabel->setText(text);
}

void ZzSftpPanel::updateAvailability()
{
    const bool open = m_ops && m_ops->isOpen();
    const bool ready = open && !m_loading;
    bool selectedIsDir = false;
    const bool hasSelection = ready && !selectedPath(&selectedIsDir).isEmpty();
    m_upButton->setEnabled(ready);
    m_refreshButton->setEnabled(ready);
    m_uploadButton->setEnabled(ready);
    m_downloadButton->setEnabled(hasSelection && !selectedIsDir);
    m_mkdirButton->setEnabled(ready);
    m_deleteButton->setEnabled(hasSelection);
    m_renameButton->setEnabled(hasSelection);
    m_pathEdit->setEnabled(open);
}

QString ZzSftpPanel::currentPath() const
{
    return m_currentPath;
}

int ZzSftpPanel::visibleEntryCount() const
{
    return m_dirModel->rowCount();
}

int ZzSftpPanel::transferRowCount() const
{
    return m_transferView->topLevelItemCount();
}

QString ZzSftpPanel::transferStatusText(quint64 requestId) const
{
    if (QTreeWidgetItem *item = m_transferRows.value(requestId)) {
        return item->text(kColStatus);
    }
    // 历史行回退：换绑后请求哈希已清空，按行内角色数据倒序查（新行优先）
    for (int i = m_transferView->topLevelItemCount() - 1; i >= 0; --i) {
        QTreeWidgetItem *item = m_transferView->topLevelItem(i);
        if (item->data(0, Qt::UserRole).toULongLong() == requestId) {
            return item->text(kColStatus);
        }
    }
    return QString();
}

QString ZzSftpPanel::statusText() const
{
    return m_statusLabel->text();
}

bool ZzSftpPanel::isLoading() const
{
    return m_loading;
}

bool ZzSftpPanel::opsAttached() const
{
    return m_ops != nullptr;
}
