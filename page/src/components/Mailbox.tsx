import { Mailbox as MailboxData } from '~/src/data'

import Address from './Address'

import './Mailbox.css'

interface Props {
    mailbox: MailboxData
    format?: 'short' | 'full'
}

export default function Mailbox({ mailbox, format = 'full' }: Props) {
    switch (format) {
    case 'full':
        return <span className="mailbox">
            {mailbox.name != null && <span className="name">{mailbox.name}</span>}
            <Address address={mailbox.address} />
        </span>

    case 'short':
        return mailbox.name == null
            ? <Address address={mailbox.address} />
            : <span className="mailbox">{mailbox.name}</span>
    }
}
