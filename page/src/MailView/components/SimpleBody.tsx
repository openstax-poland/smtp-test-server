// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import { Message, MessageData } from '~/src/data'

interface Props {
    message: Message
    part?: string
    data: MessageData
}

/** Body of a message which is not multi-part */
export default function SimpleBody({ message, part, data }: Props) {
    if (data.contentType.startsWith('text/html')) {
        const url = part == null ? `/messages/${message.id}` : `/messages/${message.id}${part}`
        return <Frame src={url} />
    } else {
        return <pre>{data.data}</pre>
    }
}

interface FrameProps {
    src: string
}

function Frame({ src }: FrameProps) {
    const ref = React.useRef<HTMLIFrameElement | null>(null)

    const resize = React.useCallback(() => resizeFrame(ref.current!), [ref])

    React.useEffect(() => {
        const observer = new ResizeObserver(resize)
        observer.observe(ref.current!)
        return () => observer.disconnect()
    }, [ref, resize])

    return <iframe ref={ref} src={src} sandbox="allow-same-origin" onLoad={resize} />
}

function resizeFrame(frame: HTMLIFrameElement) {
    frame.width = frame.contentDocument!.body.parentElement!.scrollWidth as any
    frame.height = frame.contentDocument!.body.parentElement!.scrollHeight as any
}
