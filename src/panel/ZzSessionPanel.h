#pragma once

#include <QtWidgets/QDockWidget>

#include "ZzPanelInterface.h"
#include "session/ZzSessionProfile.h"

class QStandardItemModel;
class QTreeView;
class ZzCredentialStore;
class ZzSessionModel;

/**
 * @brief 会话面板：树形分组、双击连接、右键新建/编辑/删除/复制（规格 §七）。
 *
 * 实现 ZzPanelInterface 注册进壳层；数据完全来自 ZzSessionModel，
 * 模型 sessionsChanged() 即重建树（v0.1 会话量级下重建成本可忽略）。
 */
class ZzSessionPanel : public QDockWidget, public ZzPanelInterface
{
    Q_OBJECT
public:
    explicit ZzSessionPanel(ZzSessionModel *model,
                            ZzCredentialStore *store,
                            QWidget *parent = nullptr);

    // ---- ZzPanelInterface ----
    [[nodiscard]] QString panelId() const override;
    [[nodiscard]] QString panelTitle() const override;
    [[nodiscard]] QWidget *panelWidget() override;

    // ---- 测试观察口（等价于 UI 操作，离屏环境不用模拟鼠标） ----
    /** @brief 触发连接（等价双击会话项）；id 为空等价双击分组项。 */
    void triggerConnect(const QString &profileId);
    /** @brief 触发删除（等价右键→删除）。 */
    void triggerDelete(const QString &profileId);
    /** @brief 顶层分组数（可见树）。 */
    [[nodiscard]] int visibleGroupCount() const;
    /** @brief 会话叶子总数（可见树）。 */
    [[nodiscard]] int visibleSessionCount() const;

signals:
    /** @brief 双击会话请求连接（规格 §七连接流程起点）。 */
    void connectRequested(const ZzSessionProfile &profile);

private slots:
    void rebuildTree();
    void onTreeDoubleClicked(const QModelIndex &index);
    void showContextMenu(const QPoint &pos);
    void newSession(const QString &groupPathPrefix);
    void editSession(const QString &profileId);
    void duplicateSession(const QString &profileId);

private:
    ZzSessionModel *m_model;
    ZzCredentialStore *m_store;
    QTreeView *m_tree;
    QStandardItemModel *m_treeModel;
};
