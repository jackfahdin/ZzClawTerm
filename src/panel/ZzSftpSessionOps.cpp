#include "ZzSftpSessionOps.h"

#include <ZzSftpSession.h>

ZzSftpSessionOps::ZzSftpSessionOps(ZzSftpSession *session, QObject *parent)
    : ZzSftpOps(parent)
    , m_session(session)
{
    if (!m_session) {
        return;
    }
    // 信号一对一透传（同线程 direct 连接，语义不变）
    connect(m_session, &ZzSftpSession::opened, this, &ZzSftpSessionOps::opened);
    connect(m_session, &ZzSftpSession::errorOccurred,
            this, &ZzSftpSessionOps::errorOccurred);
    connect(m_session, &ZzSftpSession::closed, this, &ZzSftpSessionOps::closed);
    connect(m_session, &ZzSftpSession::dirListed,
            this, &ZzSftpSessionOps::dirListed);
    connect(m_session, &ZzSftpSession::operationFinished,
            this, &ZzSftpSessionOps::operationFinished);
    connect(m_session, &ZzSftpSession::operationError,
            this, &ZzSftpSessionOps::operationError);
    connect(m_session, &ZzSftpSession::transferProgress,
            this, &ZzSftpSessionOps::transferProgress);
    connect(m_session, &ZzSftpSession::transferFinished,
            this, &ZzSftpSessionOps::transferFinished);
    connect(m_session, &ZzSftpSession::transferError,
            this, &ZzSftpSessionOps::transferError);
}

bool ZzSftpSessionOps::isOpen() const
{
    return m_session && m_session->isOpen();
}

quint64 ZzSftpSessionOps::listDir(const QString &path)
{
    return m_session ? m_session->listDir(path) : 0;
}

quint64 ZzSftpSessionOps::makeDir(const QString &path, long mode)
{
    return m_session ? m_session->makeDir(path, mode) : 0;
}

quint64 ZzSftpSessionOps::removeFile(const QString &path)
{
    return m_session ? m_session->removeFile(path) : 0;
}

quint64 ZzSftpSessionOps::removeDir(const QString &path)
{
    return m_session ? m_session->removeDir(path) : 0;
}

quint64 ZzSftpSessionOps::rename(const QString &oldPath, const QString &newPath)
{
    return m_session ? m_session->rename(oldPath, newPath) : 0;
}

quint64 ZzSftpSessionOps::upload(const QString &localPath, const QString &remotePath)
{
    return m_session ? m_session->upload(localPath, remotePath) : 0;
}

quint64 ZzSftpSessionOps::download(const QString &remotePath, const QString &localPath)
{
    return m_session ? m_session->download(remotePath, localPath) : 0;
}

void ZzSftpSessionOps::cancelTransfer(quint64 requestId)
{
    if (m_session) {
        m_session->cancelTransfer(requestId);
    }
}

void ZzSftpSessionOps::closeSession()
{
    if (m_session) {
        m_session->closeSession();
    }
}
