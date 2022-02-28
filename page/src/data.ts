// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

/** Single email message */
export interface Message {
    /** Message ID */
    id: string
    /** Subject */
    subject: string
    /** Sender's email address */
    from: Mailbox[]
    /** Addressee's email address */
    to: (Mailbox | Group)[]
    /** Date and time when this message was sent, as a UNIX timestamp */
    date: number,
    body: 'data' | 'mime-multipart',
}

export interface Group {
    name: string
    members: Mailbox[]
}

export interface Mailbox {
    name: string | null
    address: Address
}

export interface Address {
    local: string
    domain: string
}

/**
 * Subscribe to new messages, calling {@code onMessage} when a message arrives
 */
export function subscribe(onMessage: (message: Message) => void): () => void {
    const ws = new WebSocket(`ws://${location.host}/subscribe`)

    ws.onclose = () => console.log('connection closed')
    ws.onerror = ev => console.log('connection error:', ev)
    ws.onopen = () => console.log('connection established')
    ws.onmessage = ev => {
        const message = JSON.parse(ev.data)
        console.log('new message:', message)
        onMessage(message)
    }

    return () => ws.close()
}

/** Load list of messages */
export async function loadMessages(): Promise<Message[]> {
    const rsp = await fetch('/messages')
    return await rsp.json()
}

export interface MessageData {
    contentType: string
    data: string | Multipart
}

export interface Multipart {
    kind: 'mixed' | 'alternative'
    parts: Part[]
}

export interface Part {
    contentType: string
}

export async function loadMessage(id: string, part?: string): Promise<MessageData> {
    const url = part == null
        ? `/messages/${id}`
        : `/messages/${id}/${part}`
    const rsp = await fetch(url)

    let data

    if (rsp.headers.get('Content-Type')?.startsWith('application/json')) {
        data = await rsp.json()
    } else {
        data = await rsp.text()
    }

    return {
        contentType: rsp.headers.get('Content-Type') ?? 'application/octet-stream',
        data,
    }
}
