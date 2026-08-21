#include "ZzSplitContainer.h"

#include <climits>

#include <QtWidgets/QApplication>
#include <QtWidgets/QSplitter>
#include <QtWidgets/QVBoxLayout>

#include "terminal/ZzTerminalView.h"

ZzSplitContainer::ZzSplitContainer(QWidget *parent)
    : QWidget(parent)
{
    auto *layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    // 根分割器方向无关紧要：根内只有单子时首次异向分屏会直接改根方向
    m_root = new QSplitter(Qt::Horizontal, this);
    m_root->setChildrenCollapsible(false);
    layout->addWidget(m_root);

    // 焦点跟踪：窗格内 QTermWidget（其焦点代理到内部显示件）才是实际焦点接收者，
    // 统一经 qApp 焦点变化反查所属窗格
    connect(qApp, &QApplication::focusChanged, this,
            [this](QWidget *, QWidget *now) {
                if (!now) {
                    return;
                }
                for (ZzTerminalView *view : views()) {
                    if (now == view || view->isAncestorOf(now)) {
                        if (m_focused != view) {
                            m_focused = view;
                            emit focusedViewChanged(view);
                        }
                        return;
                    }
                }
            });
}

void ZzSplitContainer::addInitialView(ZzTerminalView *view)
{
    m_root->addWidget(view);
    m_focused = view;
}

void ZzSplitContainer::splitFocused(Qt::Orientation orientation,
                                    ZzTerminalView *newView)
{
    ZzTerminalView *anchor = focusedView();
    if (!anchor) {
        addInitialView(newView);
        return;
    }
    auto *parent = qobject_cast<QSplitter *>(anchor->parentWidget());
    if (!parent) {
        return; // 锚点不在分割树上，不应发生
    }
    if (parent == m_root && parent->count() == 1
        && parent->orientation() != orientation) {
        // 根内仅单子：直接改根方向，避免无谓的嵌套层级
        parent->setOrientation(orientation);
    }
    if (parent->orientation() == orientation) {
        parent->insertWidget(parent->indexOf(anchor) + 1, newView);
    } else {
        auto *split = new QSplitter(orientation);
        split->setChildrenCollapsible(false);
        parent->insertWidget(parent->indexOf(anchor), split);
        split->addWidget(anchor);   // insertWidget/addWidget 自动重设父对象
        split->addWidget(newView);
    }
    m_focused = newView;
    newView->setFocus(); // 经焦点代理落到 QTermWidget
    emit focusedViewChanged(newView);
}

void ZzSplitContainer::closeView(ZzTerminalView *view)
{
    if (!view || !containsView(view)) {
        return;
    }
    emit paneClosing(view);
    auto *parent = qobject_cast<QSplitter *>(view->parentWidget());
    if (m_focused == view) {
        m_focused = nullptr;
    }
    // 先摘除再销毁，使分割树同步收拢（deleteLater 的销毁发生在事件循环之后）
    view->setParent(nullptr);
    view->deleteLater();
    collapseSingleChildSplitters(parent);

    if (paneCount() == 0) {
        emit emptied();
        return;
    }
    if (!m_focused) {
        m_focused = views().first();
        m_focused->setFocus();
        emit focusedViewChanged(m_focused);
    }
}

void ZzSplitContainer::collapseSingleChildSplitters(QSplitter *parent)
{
    while (parent && parent != m_root && parent->count() == 1) {
        QWidget *child = parent->widget(0);
        auto *grand = qobject_cast<QSplitter *>(parent->parentWidget());
        if (!grand) {
            return; // 父链断裂，不应发生
        }
        const int index = grand->indexOf(parent);
        grand->insertWidget(index, child); // 唯一子件提升到祖父层原位
        parent->deleteLater();             // 已空壳的分割器随事件循环销毁
        parent = grand;
    }
}

void ZzSplitContainer::focusPane(FocusDirection direction)
{
    ZzTerminalView *from = focusedView();
    if (!from) {
        return;
    }
    const QPoint origin = from->mapTo(this, from->rect().center());
    ZzTerminalView *best = nullptr;
    int bestScore = INT_MAX;
    for (ZzTerminalView *candidate : views()) {
        if (candidate == from) {
            continue;
        }
        const QPoint center =
            candidate->mapTo(this, candidate->rect().center());
        const int dx = center.x() - origin.x();
        const int dy = center.y() - origin.y();
        int primary = 0;   ///< 主方向距离（须为正才算该方向候选）
        int secondary = 0; ///< 垂直于主方向的偏移
        switch (direction) {
        case FocusDirection::Left:  primary = -dx; secondary = qAbs(dy); break;
        case FocusDirection::Right: primary = dx;  secondary = qAbs(dy); break;
        case FocusDirection::Up:    primary = -dy; secondary = qAbs(dx); break;
        case FocusDirection::Down:  primary = dy;  secondary = qAbs(dx); break;
        }
        if (primary <= 0) {
            continue;
        }
        // 主方向距离优先（近者优先），垂直于主方向的偏移仅作次级排序键；
        // 规则分屏布局下同主方向窗格的偏移通常相同，等价于取最近邻
        const int score = primary * 1000 + secondary;
        if (score < bestScore) {
            bestScore = score;
            best = candidate;
        }
    }
    if (best) {
        best->setFocus(); // focusChanged 跟踪会更新 m_focused 并发信号
    }
}

ZzTerminalView *ZzSplitContainer::focusedView() const
{
    if (m_focused && containsView(m_focused)) {
        return m_focused;
    }
    const QList<ZzTerminalView *> all = views();
    return all.isEmpty() ? nullptr : all.first();
}

QList<ZzTerminalView *> ZzSplitContainer::views() const
{
    QList<ZzTerminalView *> result;
    QList<QSplitter *> stack{m_root};
    while (!stack.isEmpty()) {
        QSplitter *splitter = stack.takeLast();
        for (int i = 0; i < splitter->count(); ++i) {
            QWidget *child = splitter->widget(i);
            if (auto *view = qobject_cast<ZzTerminalView *>(child)) {
                result.append(view);
            } else if (auto *nested = qobject_cast<QSplitter *>(child)) {
                stack.append(nested);
            }
        }
    }
    return result;
}

int ZzSplitContainer::paneCount() const
{
    return static_cast<int>(views().size());
}

bool ZzSplitContainer::containsView(ZzTerminalView *view) const
{
    return views().contains(view);
}
