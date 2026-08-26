#include "ZzMockSftpOps.h"

#include <QtCore/QTimer>

ZzMockSftpOps::ZzMockSftpOps(QObject *parent)
    : ZzSftpOps(parent)
{
}

quint64 ZzMockSftpOps::listDir(const QString &path)
{
    if (!m_open) {
        return 0;
    }
    listedPaths.append(path);
    const quint64 id = nextReqId++;
    if (autoReplyDir) {
        const QList<ZzSftpFileInfo> entries = dirReply;
        const int code = listDirErrorCode;
        const QString message = listDirErrorMessage;
        QTimer::singleShot(0, this, [this, id, entries, code, message] {
            if (code != 0) {
                emit operationError(id, code, message); // listDir 失败也走 operationError
            } else {
                emit dirListed(id, entries);
            }
        });
    }
    return id;
}

quint64 ZzMockSftpOps::recordSimpleOp()
{
    if (!m_open) {
        return 0;
    }
    const quint64 id = nextReqId++;
    const int code = opErrorCode;
    const QString message = opErrorMessage;
    QTimer::singleShot(0, this, [this, id, code, message] {
        if (code != 0) {
            emit operationError(id, code, message);
        } else {
            emit operationFinished(id);
        }
    });
    return id;
}

quint64 ZzMockSftpOps::makeDir(const QString &path, long mode)
{
    Q_UNUSED(mode);
    madeDirs.append(path);
    return recordSimpleOp();
}

quint64 ZzMockSftpOps::removeFile(const QString &path)
{
    removedFiles.append(path);
    return recordSimpleOp();
}

quint64 ZzMockSftpOps::removeDir(const QString &path)
{
    removedDirs.append(path);
    return recordSimpleOp();
}

quint64 ZzMockSftpOps::rename(const QString &oldPath, const QString &newPath)
{
    renamed.append({oldPath, newPath});
    return recordSimpleOp();
}

quint64 ZzMockSftpOps::upload(const QString &localPath, const QString &remotePath)
{
    if (!m_open) {
        return 0;
    }
    uploaded.append({localPath, remotePath});
    return nextReqId++;
}

quint64 ZzMockSftpOps::download(const QString &remotePath, const QString &localPath)
{
    if (!m_open) {
        return 0;
    }
    downloaded.append({remotePath, localPath});
    return nextReqId++;
}

quint64 ZzMockSftpOps::uploadDir(const QString &localDir, const QString &remoteDir)
{
    if (!m_open) {
        return 0;
    }
    uploadedDirs.append({localDir, remoteDir});
    return nextReqId++;
}

quint64 ZzMockSftpOps::downloadDir(const QString &remoteDir, const QString &localDir)
{
    if (!m_open) {
        return 0;
    }
    downloadedDirs.append({remoteDir, localDir});
    return nextReqId++;
}

void ZzMockSftpOps::cancelTransfer(quint64 requestId)
{
    cancelledTransfers.append(requestId);
}

void ZzMockSftpOps::closeSession()
{
    ++closeCallCount;
}

void ZzMockSftpOps::simulateOpened()
{
    m_open = true;
    emit opened();
}

void ZzMockSftpOps::simulateClosed()
{
    m_open = false;
    emit closed();
}

void ZzMockSftpOps::simulateError(int code, const QString &message)
{
    emit errorOccurred(code, message);
}

void ZzMockSftpOps::simulateDirListed(quint64 requestId,
                                      const QList<ZzSftpFileInfo> &entries)
{
    emit dirListed(requestId, entries);
}

void ZzMockSftpOps::simulateProgress(quint64 requestId, qint64 done, qint64 total)
{
    emit transferProgress(requestId, done, total);
}

void ZzMockSftpOps::simulateTransferFinished(quint64 requestId)
{
    emit transferFinished(requestId);
}

void ZzMockSftpOps::simulateTransferError(quint64 requestId, int code,
                                          const QString &message)
{
    emit transferError(requestId, code, message);
}
