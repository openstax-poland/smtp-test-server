import * as React from 'react'

import MailList from '../MailList'

import { Message } from '../data'

import './index.css'

export default function App() {
    const [messages, setMessages] = React.useState<Message[]>([])
    const [selected, setSelected] = React.useState<Message | null>(null)

    return <>
        <MailList messages={messages} onSelect={setSelected} />
    </>
}
