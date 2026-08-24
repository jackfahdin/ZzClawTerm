#include "ZzX11Viewport.h"
#include "ZzX11ViewportPrivate.h"

#include <QPalette>
#include <QResizeEvent>

ZzX11Viewport::ZzX11Viewport(QWidget *parent)
    : QWidget(parent)
    , d_ptr(std::make_unique<ZzX11ViewportPrivate>())
{
    // server 未就绪前先铺深色背景，避免白底刺眼
    setAutoFillBackground(true);
    QPalette pal = palette();
    pal.setColor(QPalette::Window, QColor(0x1e, 0x1e, 0x1e));
    setPalette(pal);
}

ZzX11Viewport::~ZzX11Viewport() = default;

quintptr ZzX11Viewport::embeddingHandle() const
{
#ifdef Q_OS_WIN
    return static_cast<quintptr>(winId());
#else
    return 0;
#endif
}

QString ZzX11Viewport::x11WindowClassName()
{
    return QStringLiteral("ZzXsrv/x");
}

QRect ZzX11Viewport::computeFollowRect(const QSize &containerSize)
{
    return QRect(0, 0, qMax(0, containerSize.width()), qMax(0, containerSize.height()));
}

void ZzX11Viewport::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
#ifdef Q_OS_WIN
    Q_D(const ZzX11Viewport);
    d->repositionXChildWindows(embeddingHandle(), computeFollowRect(event->size()));
#else
    Q_UNUSED(event);
#endif
}
