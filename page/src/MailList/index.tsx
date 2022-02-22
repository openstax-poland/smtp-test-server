import * as React from 'react'

import DateTime from '~/src/components/DateTime'
import GroupOrMailbox from '~/src/components/GroupOrMailbox'
import Mailbox from '~/src/components/Mailbox'

import { Message } from '~/src/data'

import './index.css'

interface Props {
    messages: Message[]
    onSelect: (message: Message) => void
}

export default function MailList({ messages, onSelect }: Props) {
    const [selected, setSelected] = React.useState(null)

    const onSelectMessage = React.useCallback(message => {
        setSelected(message?.id)
        onSelect(message)
    }, [onSelect, setSelected])

    const className = selected == null ? "mail-list" : "mail-list selected"

    return <div className={className}>
        <table>
            <thead>
                <tr>
                    <th className="stretch">Subject</th>
                    <th>From</th>
                    <th>To</th>
                    <th>Date</th>
                </tr>
            </thead>
            <tbody>
                {messages.map(message => (
                    <Item
                        key={message.id}
                        selected={selected == message.id}
                        message={message}
                        onSelect={onSelectMessage}
                        />
                ))}
            </tbody>
        </table>
    </div>
}

interface ItemProps {
    message: Message
    selected: boolean
    onSelect: (message: Message | null) => void
}

function Item({ message, selected, onSelect }: ItemProps) {
    const onClick = React.useCallback(() => {
        onSelect(selected ? null : message)
    }, [selected, onSelect, message])

    return <tr className={selected ? 'selected' : undefined} onClick={onClick}>
        <td className="subject">{message.subject}</td>
        <td className="from">
            <Mailbox format="short" mailbox={message.from[0]} />
        </td>
        <td className="to">
            <GroupOrMailbox format="short" group={message.to[0]} />
        </td>
        <td className="date">
            <DateTime format="tiny" date={new Date(message.date * 1000)} />
        </td>
    </tr>
}
