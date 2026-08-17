#include "ZzHostKeyDialog.h"

#include <QtWidgets/QMessageBox>

bool ZzHostKeyDialog::confirm(const QString &host, const QString &fingerprint,
                              const QString &oldFingerprint, bool changed,
                              QWidget *parent)
{
    if (changed) {
        // 密钥变更：高危警告，默认拒绝（规格 §八）；展示旧新指纹对比
        const auto choice = QMessageBox::warning(parent,
            QStringLiteral("警告：主机密钥已变更"),
            QStringLiteral("主机 %1 的密钥指纹与本地记录不一致！\n\n"
                           "旧指纹（本地记录）：%2\n"
                           "新指纹（服务器上报）：%3\n\n"
                           "这可能意味着中间人攻击或服务器重装。"
                           "确认无误后才可继续。")
                .arg(host, oldFingerprint, fingerprint),
            QMessageBox::Yes | QMessageBox::No,
            QMessageBox::No);
        return choice == QMessageBox::Yes;
    }
    // 首次连接：指纹确认
    const auto choice = QMessageBox::question(parent,
        QStringLiteral("确认主机密钥"),
        QStringLiteral("首次连接主机 %1。\n\n密钥指纹：%2\n\n"
                       "是否信任并保存该指纹？").arg(host, fingerprint),
        QMessageBox::Yes | QMessageBox::No,
        QMessageBox::No);
    return choice == QMessageBox::Yes;
}
