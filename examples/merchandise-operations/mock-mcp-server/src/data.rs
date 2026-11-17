pub(super) const DATA_AS_OF: &str = "2026-11-16T09:30:00+08:00";
pub(super) const RULES_VERSION: &str = "mock-transfer-v1";

#[derive(Clone, Debug)]
pub(super) struct Store {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) region: String,
    pub(super) city: String,
}

#[derive(Clone, Debug)]
pub(super) struct Sku {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) category: String,
    pub(super) size: String,
    pub(super) color: String,
}

#[derive(Clone, Debug)]
pub(super) struct InventoryRecord {
    pub(super) store_index: usize,
    pub(super) sku_index: usize,
    pub(super) on_hand: u32,
    pub(super) reserved: u32,
    pub(super) sales_7d: u32,
    pub(super) sales_28d: u32,
    pub(super) inbound_7d: u32,
    pub(super) outbound_pending: u32,
    pub(super) receiving_capacity: u32,
}

impl InventoryRecord {
    pub(super) fn available(&self) -> u32 {
        self.on_hand.saturating_sub(self.reserved)
    }

    pub(super) fn average_daily_sales(&self) -> f64 {
        f64::from(self.sales_7d) / 7.0
    }

    pub(super) fn cover_days(&self) -> f64 {
        let daily = self.average_daily_sales();
        if daily == 0.0 {
            if self.available() == 0 { 0.0 } else { 999.0 }
        } else {
            f64::from(self.available()) / daily
        }
    }
}

pub(super) fn build_stores() -> Vec<Store> {
    const REGIONS: [(&str, &[&str]); 5] = [
        ("华东", &["上海", "杭州", "南京", "苏州", "宁波"]),
        ("华南", &["广州", "深圳", "佛山", "东莞", "厦门"]),
        ("华北", &["北京", "天津", "济南", "青岛", "石家庄"]),
        ("西南", &["成都", "重庆", "昆明", "贵阳", "绵阳"]),
        ("华中", &["武汉", "长沙", "郑州", "南昌", "合肥"]),
    ];
    let mut stores = Vec::with_capacity(300);
    for (region_index, (region, cities)) in REGIONS.iter().enumerate() {
        for local_index in 0..60 {
            let city = cities[local_index % cities.len()];
            let ordinal = local_index / cities.len() + 1;
            stores.push(Store {
                id: format!("ST{:03}", region_index * 60 + local_index + 1),
                name: format!("{city}直营店-{ordinal:02}"),
                region: (*region).to_owned(),
                city: city.to_owned(),
            });
        }
    }
    stores
}

pub(super) fn build_skus() -> Vec<Sku> {
    const ITEMS: [(&str, &str, &str); 24] = [
        ("AW-COAT-001", "羊毛双面大衣", "女装"),
        ("AW-DOWN-002", "轻薄羽绒服", "女装"),
        ("AW-KNIT-003", "半高领针织衫", "女装"),
        ("AW-TRENCH-004", "中长款风衣", "女装"),
        ("AW-DRESS-005", "针织长裙", "女装"),
        ("AW-SKIRT-006", "羊毛半身裙", "女装"),
        ("AW-DOWN-007", "工装羽绒服", "男装"),
        ("AW-JACKET-008", "摇粒绒夹克", "男装"),
        ("AW-SWEATER-009", "圆领羊毛衫", "男装"),
        ("AW-PANTS-010", "加厚休闲裤", "男装"),
        ("AW-SHIRT-011", "法兰绒衬衫", "男装"),
        ("AW-HOODIE-012", "连帽卫衣", "男装"),
        ("AC-SCARF-013", "羊绒围巾", "配饰"),
        ("AC-HAT-014", "针织帽", "配饰"),
        ("AC-GLOVE-015", "触屏手套", "配饰"),
        ("AC-BAG-016", "通勤托特包", "配饰"),
        ("AC-SOCK-017", "保暖中筒袜", "配饰"),
        ("AC-BELT-018", "窄版皮带", "配饰"),
        ("AW-KIDS-019", "儿童羽绒马甲", "童装"),
        ("AW-KIDS-020", "儿童针织开衫", "童装"),
        ("AW-KIDS-021", "儿童加绒长裤", "童装"),
        ("AW-KIDS-022", "儿童连帽外套", "童装"),
        ("AW-BASIC-023", "基础高领打底衫", "女装"),
        ("AW-BASIC-024", "基础圆领卫衣", "男装"),
    ];
    const SIZES: [&str; 6] = ["XS", "S", "M", "L", "XL", "均码"];
    const COLORS: [&str; 4] = ["燕麦色", "深海军蓝", "炭灰", "酒红"];
    ITEMS
        .iter()
        .enumerate()
        .map(|(index, (id, name, category))| Sku {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            category: (*category).to_owned(),
            size: SIZES[index % SIZES.len()].to_owned(),
            color: COLORS[(index * 3) % COLORS.len()].to_owned(),
        })
        .collect()
}

pub(super) fn build_inventory(store_index: usize, sku_index: usize) -> InventoryRecord {
    let demand = u32::try_from(2 + sku_index % 5).unwrap_or(2);
    let stock_noise = noise(store_index, sku_index, 11);
    let sales_noise = noise(store_index, sku_index, 23);
    let transit_noise = noise(store_index, sku_index, 37);
    let hot_store = store_index.is_multiple_of(7);
    let excess_store = store_index.is_multiple_of(11);
    let sales_7d = sales_noise % (demand * 4 + 2) + u32::from(hot_store);
    let mut on_hand = stock_noise % 42 + demand * 2;
    if excess_store {
        on_hand += 55 + stock_noise % 30;
    }
    if hot_store {
        on_hand = 3 + stock_noise % (10 + demand * 2);
    }
    let reserved = stock_noise % on_hand.saturating_add(1).min(9);
    InventoryRecord {
        store_index,
        sku_index,
        on_hand,
        reserved,
        sales_7d,
        sales_28d: sales_7d * 3 + sales_noise % 9,
        inbound_7d: transit_noise % 10,
        outbound_pending: noise(store_index, sku_index, 53) % 5,
        receiving_capacity: 12 + noise(store_index, sku_index, 71) % 58,
    }
}

fn noise(store_index: usize, sku_index: usize, salt: usize) -> u32 {
    let mut value = store_index
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add(sku_index.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(salt.wrapping_mul(0xc2b2_ae35));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    u32::try_from(value % 10_000).unwrap_or(0)
}
