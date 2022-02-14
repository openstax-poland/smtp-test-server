import * as React from 'react'

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

    return <div className="mail-list">
        <table className="mail-list">
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
        <td className="from">{message.from}</td>
        <td className="to">{message.to}</td>
        <td className="date">date</td>
    </tr>
}
