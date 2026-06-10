/**
 * @file ZzSearchBar.cpp
 * @brief ZzSearchBar 公开接口实现。
 */

#include "ui/ZzSearchBar.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>

#include "ui/ZzSearchBarPrivate.h"

namespace ZzUi {

ZzSearchBar::ZzSearchBar(QWidget* parent)
    : QWidget(parent), d_ptr(std::make_unique<ZzSearchBarPrivate>())
{
    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(4, 2, 4, 2);

    d_ptr->input = new QLineEdit(this);
    d_ptr->input->setPlaceholderText(tr("搜索日志…"));
    d_ptr->countLabel = new QLabel(this);
    auto* closeButton = new QPushButton(tr("关闭"), this);

    layout->addWidget(new QLabel(tr("查找:"), this));
    layout->addWidget(d_ptr->input, 1);
    layout->addWidget(d_ptr->countLabel);
    layout->addWidget(closeButton);

    connect(d_ptr->input, &QLineEdit::returnPressed, this,
            [this]() { emit searchRequested(d_ptr->input->text()); });
    connect(closeButton, &QPushButton::clicked, this, [this]() {
        hide();
        emit closed();
    });

    hide();
}

ZzSearchBar::~ZzSearchBar() = default;

void ZzSearchBar::activate()
{
    show();
    d_ptr->input->setFocus();
    d_ptr->input->selectAll();
}

void ZzSearchBar::setResultCount(int count)
{
    d_ptr->countLabel->setText(tr("%1 条匹配").arg(count));
}

}  // namespace ZzUi
