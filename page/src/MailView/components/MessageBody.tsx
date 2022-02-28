// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import { Message, MessageData, loadMessage } from '~/src/data'

import SimpleBody from './SimpleBody'
import MultipartBody from './MultipartBody'

interface Props {
    message: Message
    part?: string
}

export default function MessageBody({ message, part }: Props) {
    const [body, setBody] = React.useState<MessageData | null>(null)

    React.useEffect(() => {
        loadMessage(message.id, part).then(setBody)
    }, [message.id, part, setBody])

    if (body == null) {
        return <div>Loading</div>
    }

    if (typeof body.data === 'string') {
        return <SimpleBody message={message} part={part} data={body} />
    } else {
        return <MultipartBody message={message} part={part} data={body.data} />
    }
}
