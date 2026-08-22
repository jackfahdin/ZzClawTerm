#pragma once

#include <QString>

/**
 * @brief X11 forwarding 授权管理：cookie 生成与 xauthority 文件写入。
 *
 * 应用侧职责：SSH X11 forwarding 建立前，为本地 X server 生成 MIT-MAGIC-COOKIE-1
 * 授权记录——Windows 下写入独立 xauthority 文件供转发通道校验，Linux/macOS 下
 * 通过系统 xauth 直接并入用户授权库。
 */
class ZzXAuthority
{
public:
    /** @brief 生成 32 字符十六进制 cookie（16 字节加密随机）。 */
    QString generateCookie() const;

    /**
     * @brief 以 xauth 二进制格式写入授权记录（family=FamilyWild，0600 权限，QSaveFile 原子落盘）。
     * @param path 目标文件路径（已存在则整体替换）。
     * @param display X 显示号（如 10 对应 localhost:10.0）。
     * @param cookieHex 32 字符十六进制 cookie，长度不符时返回 false。
     * @return 写入成功（含权限设置）返回 true。
     */
    bool writeXauthorityFile(const QString &path, int display, const QString &cookieHex) const;

    /**
     * @brief Linux/macOS：对系统 X server 执行 xauth add（$XAUTHORITY 或默认 ~/.Xauthority）。
     * @param display X 显示号。
     * @param cookieHex 32 字符十六进制 cookie。
     * @param errorOut 失败时的错误描述（可空）。
     * @return xauth 正常退出（exit code 0）返回 true；启动失败、超时（5s）或非零退出返回 false。
     */
    bool addToSystemAuthority(int display, const QString &cookieHex, QString *errorOut = nullptr) const;
};
