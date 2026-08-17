#pragma once

#include <QtWidgets/QDialog>

/**
 * @brief 主机密钥确认对话框（规格 §八安全底线：首次确认、变更警告，不可省略）。
 *
 * 这是"不弹窗轰炸"原则的唯一例外——主机密钥变更可能意味着中间人攻击，
 * 必须显式打断用户确认。
 */
class ZzHostKeyDialog : public QDialog
{
    Q_OBJECT
public:
    /**
     * @brief 弹窗确认主机密钥。
     * @param host 主机标识（host:port）。
     * @param fingerprint 密钥指纹（SHA256 文本）。
     * @param changed true 表示与 known_hosts 中记录不一致（高危警告样式）。
     * @param parent 父窗口。
     * @return true 接受并写入 known_hosts.json；false 拒绝（连接中止）。
     */
    static bool confirm(const QString &host, const QString &fingerprint,
                        bool changed, QWidget *parent = nullptr);
};
