#pragma once

#include <QtCore/QList>
#include <QtCore/QPair>
#include <QtCore/QStringList>

#include "panel/ZzSftpOps.h"

/**
 * @brief 测试用 mock SFTP 操作：记录全部调用，脚本化应答，可手动注入事件。
 *
 * 默认行为：listDir 下一拍事件循环回 dirListed(dirReply)；简单操作
 * （makeDir/remove/rename）下一拍回 operationFinished（opErrorCode != 0 时回
 * operationError(opErrorCode, opErrorMessage)）；upload/download 只记录不发
 * 结局信号（由测试经 simulate* 驱动进度与结局）。closed/open 状态由
 * simulateOpened()/simulateClosed() 控制。
 */
class ZzMockSftpOps : public ZzSftpOps
{
    Q_OBJECT
public:
    explicit ZzMockSftpOps(QObject *parent = nullptr);

    [[nodiscard]] bool isOpen() const override { return m_open; }
    quint64 listDir(const QString &path) override;
    quint64 makeDir(const QString &path, long mode = 0755) override;
    quint64 removeFile(const QString &path) override;
    quint64 removeDir(const QString &path) override;
    quint64 rename(const QString &oldPath, const QString &newPath) override;
    quint64 upload(const QString &localPath, const QString &remotePath) override;
    quint64 download(const QString &remotePath, const QString &localPath) override;
    void cancelTransfer(quint64 requestId) override;
    void closeSession() override;

    /** @brief 记录传输块大小设置（M6 接线断言用）。 */
    void setTransferBlockSize(int bytes) override { m_blockSizes.append(bytes); }
    /** @brief 历次 setTransferBlockSize 收到的字节数。 */
    [[nodiscard]] QList<int> recordedBlockSizes() const { return m_blockSizes; }

    // ---- 事件注入 ----
    /** @brief 置为已打开并发射 opened()。 */
    void simulateOpened();
    /** @brief 发射 closed()。 */
    void simulateClosed();
    /** @brief 注入一次会话级错误。 */
    void simulateError(int code, const QString &message);
    /** @brief 注入一次目录列举应答（autoReplyDir=false 时手工驱动）。 */
    void simulateDirListed(quint64 requestId, const QList<ZzSftpFileInfo> &entries);
    /** @brief 注入一次传输进度。 */
    void simulateProgress(quint64 requestId, qint64 done, qint64 total);
    /** @brief 注入一次传输完成。 */
    void simulateTransferFinished(quint64 requestId);
    /** @brief 注入一次传输失败/取消。 */
    void simulateTransferError(quint64 requestId, int code, const QString &message);

    // ---- 脚本与记录 ----
    QList<ZzSftpFileInfo> dirReply;       ///< listDir 应答内容
    bool autoReplyDir = true;             ///< listDir 是否自动应答
    int listDirErrorCode = 0;             ///< 非 0 时 listDir 回 operationError
    QString listDirErrorMessage;          ///< listDir 失败文案
    int opErrorCode = 0;                  ///< 非 0 时简单操作回 operationError
    QString opErrorMessage;               ///< operationError 文案
    quint64 nextReqId = 1;                ///< 下一个请求 ID

    QStringList listedPaths;              ///< listDir 调用路径（含 0 返回前的）
    QStringList madeDirs;                 ///< makeDir 路径
    QStringList removedFiles;             ///< removeFile 路径
    QStringList removedDirs;              ///< removeDir 路径
    QList<QPair<QString, QString>> renamed;      ///< rename（旧，新）
    QList<QPair<QString, QString>> uploaded;     ///< upload（本地，远端）
    QList<QPair<QString, QString>> downloaded;   ///< download（远端，本地）
    QList<quint64> cancelledTransfers;    ///< cancelTransfer 请求 ID
    int closeCallCount = 0;               ///< closeSession 调用次数

private:
    /** @brief 简单操作通用应答：按 opErrorCode 回 finished 或 error。 */
    quint64 recordSimpleOp();

    bool m_open = false;
    QList<int> m_blockSizes;              ///< setTransferBlockSize 收到的字节数
};
