// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import Tabs from '~/src/components/Tabs'

import { Message, Multipart } from '~/src/data'

import MessageBody from './MessageBody'

export interface Props {
    message: Message
    part?: string
    data: Multipart
}

export default function MultipartAlternative({ message, part, data }: Props) {
    const tabs = React.useMemo(() => data.parts.map(part => {
        let title

        if (part.contentType.startsWith('text/plain')) {
            title = 'Text'
        } else if (part.contentType.startsWith('text/html')) {
            title = 'HTML'
        } else {
            title = part.contentType.split(';', 1)[0]
        }

        return { title, data: part }
    }), [data.parts])

    const render = React.useCallback((index, data) => <MessageBody
        message={message}
        part={`${part ?? ''}/${index}`}
        />,
        [message, part]
    )

    return <div className="multipart alternative">
        <Tabs tabs={tabs} render={render} selected={data.parts.length - 1} />
    </div>
}
