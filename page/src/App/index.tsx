// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import MailList from '../MailList'
import MailView from '../MailView'

import { Message, subscribe, loadMessages } from '../data'

import './index.css'

export default function App() {
    const [messages, setMessages] = React.useState<Message[]>([])
    const [selected, setSelected] = React.useState<Message | null>(null)

    React.useEffect(() => {
        loadMessages().then(messages => setMessages(messages))

        return subscribe(message => setMessages(messages => [...messages, message]))
    }, [setMessages])
    console.log(messages)

    return <>
        <MailList messages={messages} onSelect={setSelected} />
        {selected != null && <MailView message={selected} />}
    </>
}
