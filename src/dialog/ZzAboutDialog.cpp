#include "ZzAboutDialog.h"

#include <QtCore/QEvent>
#include <QtGui/QIcon>
#include <QtGui/QPixmap>
#include <QtWidgets/QDialogButtonBox>
#include <QtWidgets/QLabel>
#include <QtWidgets/QVBoxLayout>

ZzAboutDialog::ZzAboutDialog(QWidget *parent)
    : QDialog(parent)
{
    auto *layout = new QVBoxLayout(this);
    auto *iconLabel = new QLabel(this);
    iconLabel->setPixmap(QIcon(QStringLiteral(":/icons/zzclawterm.png"))
                             .pixmap(64, 64));
    iconLabel->setAlignment(Qt::AlignHCenter);
    layout->addWidget(iconLabel);

    auto *nameLabel = new QLabel(QStringLiteral("ZzClawTerm"), this);
    nameLabel->setObjectName(QStringLiteral("aboutNameLabel"));
    QFont nameFont = nameLabel->font();
    nameFont.setPointSize(nameFont.pointSize() + 2);
    nameFont.setBold(true);
    nameLabel->setFont(nameFont);
    nameLabel->setAlignment(Qt::AlignHCenter);
    layout->addWidget(nameLabel);

    m_versionLine = tr("版本 %1 · %2 构建 · %3")
        .arg(QStringLiteral("0.1.0"),
             QString::fromLatin1(ZZ_BUILD_TYPE),
             QString::fromLatin1(ZZ_GIT_REVISION).left(7));
    auto *versionLabel = new QLabel(m_versionLine, this);
    versionLabel->setObjectName(QStringLiteral("aboutVersionLabel"));
    versionLabel->setAlignment(Qt::AlignHCenter);
    layout->addWidget(versionLabel);

    auto *qtLabel = new QLabel(
        tr("基于 Qt %1").arg(QString::fromLatin1(qVersion())), this);
    qtLabel->setAlignment(Qt::AlignHCenter);
    layout->addWidget(qtLabel);

    auto *linkLabel = new QLabel(
        QStringLiteral("<a href=\"https://github.com/jackfahdin/ZzClawTerm\">"
                       "github.com/jackfahdin/ZzClawTerm</a>"), this);
    linkLabel->setOpenExternalLinks(true);
    linkLabel->setAlignment(Qt::AlignHCenter);
    layout->addWidget(linkLabel);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    retranslateUi();
}

void ZzAboutDialog::retranslateUi()
{
    setWindowTitle(tr("关于 ZzClawTerm"));
}

void ZzAboutDialog::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::LanguageChange) {
        retranslateUi();
    }
    QDialog::changeEvent(event);
}

QString ZzAboutDialog::versionLine() const
{
    return m_versionLine;
}
