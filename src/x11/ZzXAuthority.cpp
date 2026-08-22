#include "ZzXAuthority.h"

#include <QFile>
#include <QProcess>
#include <QRandomGenerator>
#include <QSaveFile>

namespace {

/// X11 协议 FamilyWild：免主机名匹配，任意地址均可凭 cookie 通过校验。
constexpr quint16 kFamilyWild = 256;

/// 授权协议名（MIT-MAGIC-COOKIE-1）。
const QByteArray kAuthName = QByteArrayLiteral("MIT-MAGIC-COOKIE-1");

} // namespace

QString ZzXAuthority::generateCookie() const
{
    QByteArray bytes(16, Qt::Uninitialized);
    QRandomGenerator::system()->fillRange(reinterpret_cast<quint32 *>(bytes.data()), 4);
    return QString::fromLatin1(bytes.toHex());
}

bool ZzXAuthority::writeXauthorityFile(const QString &path, int display, const QString &cookieHex) const
{
    const QByteArray cookie = QByteArray::fromHex(cookieHex.toLatin1());
    if (cookie.size() != 16)
        return false;

    // xauthority 记录：大端 u16 长度前缀字段
    // family(u16) + addrlen+addr + numlen+num + namelen+name + datalen+data
    QByteArray buf;
    buf.reserve(64);
    const auto putU16 = [&buf](quint16 v) {
        buf.append(char(v >> 8));
        buf.append(char(v & 0xff));
    };
    const auto putField = [&putU16, &buf](const QByteArray &d) {
        putU16(quint16(d.size()));
        buf.append(d);
    };
    putU16(kFamilyWild);
    putField(QByteArray());                       // FamilyWild 地址为空
    putField(QByteArray::number(display));        // 显示号字符串
    putField(kAuthName);
    putField(cookie);

    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly))
        return false;
    if (file.write(buf) != buf.size())
        return false;
    if (!file.commit())
        return false;

    // cookie 等同于口令，必须仅属主可读写
    return QFile::setPermissions(path, QFileDevice::ReadOwner | QFileDevice::WriteOwner);
}

bool ZzXAuthority::addToSystemAuthority(int display, const QString &cookieHex, QString *errorOut) const
{
    // xauth 默认按 $XAUTHORITY 或 ~/.Xauthority 定位授权库，无需显式 -f
    QProcess p;
    p.start(QStringLiteral("xauth"),
            {QStringLiteral("add"), QStringLiteral(":%1").arg(display),
             QStringLiteral("."), cookieHex});
    if (!p.waitForFinished(5000)) {
        if (errorOut)
            *errorOut = p.error() == QProcess::FailedToStart
                            ? QStringLiteral("xauth 启动失败：%1").arg(p.errorString())
                            : QStringLiteral("xauth 执行超时（5s）");
        return false;
    }
    if (p.exitStatus() != QProcess::NormalExit || p.exitCode() != 0) {
        if (errorOut)
            *errorOut = QStringLiteral("xauth add 失败（exit=%1）：%2")
                            .arg(p.exitCode())
                            .arg(QString::fromUtf8(p.readAllStandardError()));
        return false;
    }
    return true;
}
