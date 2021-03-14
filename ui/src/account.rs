use qmetaobject::*;

#[derive(QGadget, Clone, Default)]
#[allow(non_snake_case)]
pub struct Account {
    pub toxId: qt_property!(QString),
    pub publicKey: qt_property!(QString),
    pub name: qt_property!(QString),
}

impl From<&tocks::AccountData> for Account {
    fn from(data: &tocks::AccountData) -> Self {
        Account {
            toxId: data.tox_id().to_string().into(),
            publicKey: data.public_key().to_string().into(),
            name: data.name().into(),
        }
    }
}
