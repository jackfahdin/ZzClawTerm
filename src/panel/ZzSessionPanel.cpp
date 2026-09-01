#include "ZzSessionPanel.h"

#include <functional>
#include <optional>

#include <QtCore/QUuid>
#include <QtGui/QStandardItemModel>
#include <QtWidgets/QMenu>
#include <QtWidgets/QTreeView>
#include <QtWidgets/QVBoxLayout>

#include "ZzSessionEditDialog.h"
#include "session/ZzSessionModel.h"

namespace {

/** @brief 树节点角色键：会话 profile id（分组项无此数据）。 */
constexpr int kProfileIdRole = Qt::UserRole + 1;

} // namespace

ZzSessionPanel::ZzSessionPanel(ZzSessionModel *model,
                               ZzCredentialStore *store,
                               QWidget *parent)
    : QWidget(parent)
    , m_model(model)
    , m_store(store)
{
    setObjectName(panelId()); // 稳定标识：登记册与工作区注册共用

    m_treeModel = new QStandardItemModel(this);
    m_tree = new QTreeView(this);
    m_tree->setModel(m_treeModel);
    m_tree->setHeaderHidden(true);
    m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(m_tree);

    connect(m_tree, &QTreeView::doubleClicked,
            this, &ZzSessionPanel::onTreeDoubleClicked);
    connect(m_tree, &QTreeView::customContextMenuRequested,
            this, &ZzSessionPanel::showContextMenu);
    connect(m_model, &ZzSessionModel::sessionsChanged,
            this, &ZzSessionPanel::rebuildTree);
    rebuildTree();
}

QString ZzSessionPanel::panelId() const
{
    return QStringLiteral("sessions");
}

QString ZzSessionPanel::panelTitle() const
{
    return QStringLiteral("会话");
}

QWidget *ZzSessionPanel::panelWidget()
{
    return this;
}

void ZzSessionPanel::triggerConnect(const QString &profileId)
{
    if (profileId.isEmpty()) {
        return; // 分组项不触发
    }
    const std::optional<ZzSessionProfile> profile =
        m_model->session(QUuid::fromString(profileId));
    if (profile.has_value()) {
        emit connectRequested(*profile);
    }
}

void ZzSessionPanel::triggerDelete(const QString &profileId)
{
    m_model->removeSession(QUuid::fromString(profileId));
}

int ZzSessionPanel::visibleGroupCount() const
{
    // 只统计不带 profile id 的顶层项（未分组会话与会话同层，不算分组）
    int count = 0;
    for (int row = 0; row < m_treeModel->rowCount(); ++row) {
        if (!m_treeModel->item(row)->data(kProfileIdRole).isValid()) {
            ++count;
        }
    }
    return count;
}

int ZzSessionPanel::visibleSessionCount() const
{
    int count = 0;
    // 递归统计带 profile id 的叶子项
    const std::function<void(QStandardItem *)> walk = [&](QStandardItem *item) {
        for (int row = 0; row < item->rowCount(); ++row) {
            QStandardItem *child = item->child(row);
            if (child->data(kProfileIdRole).isValid()) {
                ++count;
            } else {
                walk(child);
            }
        }
    };
    walk(m_treeModel->invisibleRootItem());
    return count;
}

void ZzSessionPanel::rebuildTree()
{
    m_treeModel->clear();
    // 分组即路径字符串（规格 §6.1）：按 "/" 拆层建组，重命名分组=改前缀
    QHash<QString, QStandardItem *> groupItems;
    for (const ZzSessionProfile &profile : m_model->allSessions()) {
        QStandardItem *parentItem = m_treeModel->invisibleRootItem();
        QString accumulated;
        const QStringList segments = profile.groupPath.split(
            QLatin1Char('/'), Qt::SkipEmptyParts);
        for (const QString &segment : segments) {
            accumulated = accumulated.isEmpty()
                ? segment : accumulated + QLatin1Char('/') + segment;
            QStandardItem *&group = groupItems[accumulated];
            if (!group) {
                group = new QStandardItem(segment);
                group->setEditable(false);
                parentItem->appendRow(group);
            }
            parentItem = group;
        }
        auto *sessionItem = new QStandardItem(profile.name);
        sessionItem->setEditable(false);
        sessionItem->setData(profile.id.toString(QUuid::WithoutBraces),
                             kProfileIdRole);
        sessionItem->setToolTip(profile.protocol == QStringLiteral("local")
            ? QStringLiteral("本地 Shell")
            : QStringLiteral("%1@%2:%3").arg(profile.userName, profile.host)
                  .arg(profile.port));
        parentItem->appendRow(sessionItem);
    }
    m_tree->expandAll();
}

void ZzSessionPanel::onTreeDoubleClicked(const QModelIndex &index)
{
    triggerConnect(index.data(kProfileIdRole).toString());
}

void ZzSessionPanel::showContextMenu(const QPoint &pos)
{
    const QModelIndex index = m_tree->indexAt(pos);
    QMenu menu(this);
    QAction *result = nullptr;
    if (!index.isValid()) {
        // 空白区：新建会话
        QAction *newAction = menu.addAction(QStringLiteral("新建会话"));
        result = menu.exec(m_tree->viewport()->mapToGlobal(pos));
        if (result == newAction) {
            newSession(QString());
        }
        return;
    }
    const QString profileId = index.data(kProfileIdRole).toString();
    if (profileId.isEmpty()) {
        // 分组项：在此分组下新建
        QAction *newAction =
            menu.addAction(QStringLiteral("在此分组新建会话"));
        result = menu.exec(m_tree->viewport()->mapToGlobal(pos));
        if (result == newAction) {
            // 分组的完整路径 = 逐层标题拼接
            QStringList segments;
            for (QModelIndex it = index; it.isValid(); it = it.parent()) {
                segments.prepend(it.data().toString());
            }
            newSession(segments.join(QLatin1Char('/')));
        }
        return;
    }
    // 会话项：新建/编辑/删除/复制（规格 §七）
    QAction *newAction = menu.addAction(QStringLiteral("新建会话"));
    QAction *editAction = menu.addAction(QStringLiteral("编辑"));
    QAction *deleteAction = menu.addAction(QStringLiteral("删除"));
    QAction *duplicateAction = menu.addAction(QStringLiteral("复制"));
    result = menu.exec(m_tree->viewport()->mapToGlobal(pos));
    if (result == newAction) {
        newSession(QString());
    } else if (result == editAction) {
        editSession(profileId);
    } else if (result == deleteAction) {
        triggerDelete(profileId);
    } else if (result == duplicateAction) {
        duplicateSession(profileId);
    }
}

void ZzSessionPanel::newSession(const QString &groupPathPrefix)
{
    ZzSessionEditDialog dialog(m_store, {}, groupPathPrefix, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    // id 留空，由 ZzSessionModel::addSession 生成
    m_model->addSession(dialog.profile());
}

void ZzSessionPanel::editSession(const QString &profileId)
{
    const std::optional<ZzSessionProfile> existing =
        m_model->session(QUuid::fromString(profileId));
    if (!existing.has_value()) {
        return;
    }
    ZzSessionEditDialog dialog(m_store, *existing, QString(), this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    m_model->updateSession(dialog.profile());
}

void ZzSessionPanel::duplicateSession(const QString &profileId)
{
    const std::optional<ZzSessionProfile> source =
        m_model->session(QUuid::fromString(profileId));
    if (!source.has_value()) {
        return;
    }
    ZzSessionProfile copy = *source;
    copy.id = QUuid(); // 置空，由 addSession 重新生成
    copy.name = copy.name + QStringLiteral("（副本）");
    // 密码引用随副本共享同一条凭据记录，语义正确（同一台机器同一个密码）
    m_model->addSession(copy);
}
