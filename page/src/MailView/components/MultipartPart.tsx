// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import { Message, messageUrl } from '~/src/data'

import MessageBody from './MessageBody'

export interface Props {
    message: Message
    part?: string
    contentType: string
}

export default function MultipartPart({ message, part, contentType }: Props) {
    if (contentType.startsWith('text/') || contentType.startsWith('multipart/')) {
        return <MessageBody message={message} part={part} />
    }

    if (contentType.startsWith('image/')) {
        return <img src={messageUrl(message.id, part)} />
    }

    return <div>Unsupported media type {contentType}</div>
}
