use qmetaobject::*;

#[derive(QGadget, Clone, Default)]
#[allow(non_snake_case)]
pub struct Account {
    pub id: qt_property!(i64),
    pub userHandle: qt_property!(i64),
    pub toxId: qt_property!(QString),
    pub name: qt_property!(QString),
}
