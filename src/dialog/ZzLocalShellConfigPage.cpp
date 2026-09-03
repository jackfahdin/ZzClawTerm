#include "ZzLocalShellConfigPage.h"

#include <QtCore/QEvent>
#include <QtCore/QItemSelectionModel>
#include <QtGui/QStandardItemModel>
#include <QtWidgets/QFormLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QLineEdit>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QStackedWidget>
#include <QtWidgets/QTreeView>

namespace {
/** @brief 树节点/栈页索引常量（顺序即 UI 顺序）。 */
enum ZzLocalShellPageIndex {
    GeneralPage = 0,
    PageCount = 1
};
} // namespace

ZzLocalShellConfigPage::ZzLocalShellConfigPage(QWidget *parent)
    : QWidget(parent)
{
    auto *layout = new QHBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    auto *splitter = new QSplitter(this);
    layout->addWidget(splitter);

    // 左侧树：单列单节点，不可编辑、无表头、选中即切页（与 SSH 页同款结构）
    m_navTree = new QTreeView(splitter);
    m_navTree->setObjectName(QStringLiteral("localNavTree"));
    m_navTree->setHeaderHidden(true);
    m_navTree->setEditTriggers(QAbstractItemView::NoEditTriggers);
    auto *treeModel = new QStandardItemModel(0, 1, m_navTree);
    for (int i = 0; i < PageCount; ++i) {
        treeModel->appendRow(new QStandardItem); // 文本由 retranslateUi 设置
    }
    m_navTree->setModel(treeModel);
    m_navTree->setMinimumWidth(140);
    m_navTree->setMaximumWidth(200);

    m_stack = new QStackedWidget(splitter);
    splitter->setStretchFactor(1, 1);

    // —— 0 常规：名称 / 分组路径 / Shell 程序 ——
    auto *generalPage = new QWidget(this);
    auto *generalForm = new QFormLayout(generalPage);
    m_nameEdit = new QLineEdit(generalPage);
    m_nameEdit->setObjectName(QStringLiteral("nameEdit"));
    m_groupEdit = new QLineEdit(generalPage);
    m_shellEdit = new QLineEdit(generalPage);
    m_shellEdit->setObjectName(QStringLiteral("shellEdit"));
    // 行标签须显式创建空 QLabel（同 ZzSshConfigPage：空串重载不建标签部件）
    generalForm->addRow(new QLabel(generalPage), m_nameEdit);
    generalForm->addRow(new QLabel(generalPage), m_groupEdit);
    generalForm->addRow(new QLabel(generalPage), m_shellEdit);
    m_stack->addWidget(generalPage);

    connect(m_navTree->selectionModel(), &QItemSelectionModel::currentRowChanged,
            this, [this](const QModelIndex &current, const QModelIndex &) {
        if (current.isValid()) {
            m_stack->setCurrentIndex(current.row());
        }
    });

    retranslateUi();
    m_navTree->setCurrentIndex(treeModel->index(0, 0));
}

void ZzLocalShellConfigPage::setProfile(const ZzSessionProfile &profile)
{
    m_nameEdit->setText(profile.name);
    m_groupEdit->setText(profile.groupPath);
    // 契约约定：local 会话的 shell 程序路径存于 host 字段
    m_shellEdit->setText(profile.protocol == QStringLiteral("local")
                             ? profile.host : QString());
}

void ZzLocalShellConfigPage::applyTo(ZzSessionProfile &profile) const
{
    profile.name = m_nameEdit->text().trimmed();
    profile.groupPath = m_groupEdit->text().trimmed();
    profile.protocol = QStringLiteral("local");
    profile.host = m_shellEdit->text().trimmed();
}

bool ZzLocalShellConfigPage::validateInputs(QString *error, int *pageIndex) const
{
    // Shell 路径留空=系统默认，无需校验；只查名称非空（现有行为）
    if (m_nameEdit->text().trimmed().isEmpty()) {
        *error = tr("名称不能为空");
        *pageIndex = GeneralPage;
        return false;
    }
    return true;
}

void ZzLocalShellConfigPage::focusPage(int pageIndex)
{
    if (pageIndex < 0 || pageIndex >= PageCount) {
        return;
    }
    m_navTree->setCurrentIndex(m_navTree->model()->index(pageIndex, 0));
    m_stack->setCurrentIndex(pageIndex);
}

void ZzLocalShellConfigPage::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QWidget::changeEvent(event);
}

void ZzLocalShellConfigPage::retranslateUi()
{
    // 树节点文本（顺序即 ZzLocalShellPageIndex）
    auto *treeModel = qobject_cast<QStandardItemModel *>(m_navTree->model());
    if (auto *item = treeModel->item(GeneralPage, 0)) {
        item->setText(tr("常规"));
    }

    // QFormLayout 行标签按字段部件反查（labelForField，同 ZzSshConfigPage 式）
    const auto setRowLabel = [](QFormLayout *form, QWidget *field,
                                const QString &text) {
        if (auto *label = qobject_cast<QLabel *>(form->labelForField(field))) {
            label->setText(text);
        }
    };
    auto *generalForm =
        qobject_cast<QFormLayout *>(m_stack->widget(GeneralPage)->layout());
    setRowLabel(generalForm, m_nameEdit, tr("名称："));
    setRowLabel(generalForm, m_groupEdit, tr("分组路径："));
    setRowLabel(generalForm, m_shellEdit, tr("Shell 程序："));

    m_groupEdit->setPlaceholderText(tr("如：生产环境/Web 服务器"));
    m_shellEdit->setPlaceholderText(tr("留空使用系统默认 shell"));
}
