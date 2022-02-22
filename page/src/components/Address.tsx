import { Address as AddressData } from '~/src/data'

interface Props {
    address: AddressData
}

export default function Address({ address }: Props) {
    return <span className="address">
        <span className="local">{address.local}</span>
        @
        <span className="domain">{address.domain}</span>
    </span>
}
