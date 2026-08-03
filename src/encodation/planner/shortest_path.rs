use flagset::FlagSet;

use super::Plan;
use crate::{encodation::encodation_type::EncodationType, symbol_size::SymbolList};

use super::generic::GenericPlan;

#[cfg(test)]
use alloc::vec;
use alloc::vec::Vec;

#[cfg(test)]
use pretty_assertions::assert_eq;

/// En uygun encodation planını bulur.
///
/// # Argümanlar
///
/// - Input karakterlerinin kalan kısmı `data` içinde verilir.
/// - `written`, şimdiye kadar üretilen codeword sayısını (encoding uzunluğunu) belirtir.
/// - `mode`, geçerli encodation mode'udur.
/// - `symbol_list`, sonucun kullanabileceği symbol size kümesidir.
/// - `enabled_modes`, planın geçebileceği encodation mode'larını sınırlar.
pub(crate) fn optimize(
    data: &[u8],
    written: usize,
    mode: EncodationType,
    symbol_list: &SymbolList,
    enabled_modes: FlagSet<EncodationType>,
) -> Option<Vec<(usize, EncodationType)>> {
    let mut plan = optimize_plan(data, written, mode, symbol_list, enabled_modes)?;
    plan.switches.push((0, plan.current()));

    // Henüz başlangıçtaysak ASCII'ye "switch" kaydını kaldırır.
    if written == 0 && plan.switches.first().copied() == Some((data.len(), EncodationType::Ascii)) {
        plan.switches.remove(0);
    }

    Some(plan.switches)
}

/// En uygun planın kullanacağı data codeword sayısı.
///
/// [`optimize`] planının en aza indirdiği cost budur. Gerçek encoder'ın padding
/// öncesinde tam olarak bu sayıda codeword üretmesi beklenir; tutarlılık için
/// proptest'lere bakın.
#[cfg(test)]
pub(crate) fn optimize_cost(
    data: &[u8],
    written: usize,
    mode: EncodationType,
    symbol_list: &SymbolList,
    enabled_modes: FlagSet<EncodationType>,
) -> Option<usize> {
    optimize_plan(data, written, mode, symbol_list, enabled_modes)
        .map(|plan| plan.cost().ceil_codewords())
}

/// Shortest-path aramasını çalıştırır ve `switches` encoder için kesinleşmeden
/// önce kazanan planı döndürür.
fn optimize_plan<'a>(
    data: &'a [u8],
    written: usize,
    mode: EncodationType,
    symbol_list: &'a SymbolList,
    enabled_modes: FlagSet<EncodationType>,
) -> Option<GenericPlan<'a>> {
    let start_plan = GenericPlan::for_mode(mode, data, written, symbol_list);

    let mut plans = Vec::with_capacity(36);
    let mut new_plan = Vec::with_capacity(36);

    if enabled_modes.contains(mode) {
        plans.push(start_plan);
    } else {
        start_plan.add_switches(&mut plans, data.len(), true, enabled_modes);
    }

    for iteration in 0usize.. {
        let mut at_end = false;
        let use_as_start = iteration == 0;

        let rest_chars = data.len().checked_sub(iteration)?;
        for mut plan in plans.drain(0..) {
            let plan_copy_before_step = plan.clone();
            let result = if let Some(result) = plan.step() {
                result
            } else {
                plan_copy_before_step.add_switches(
                    &mut new_plan,
                    rest_chars, // Kalan karakterler
                    use_as_start,
                    enabled_modes,
                );
                // Input'u işleyemeyen planı kaldırır.
                continue;
            };
            new_plan.push(plan);

            // Adım en uygun (iyileştirilemez) değilse ve sonda değilsek diğer
            // bütün mode'lara mode switch ekleriz.
            if !result.unbeatable && !result.end {
                // Bu işlem step() yöntemini de bir kez çağırır.
                plan_copy_before_step.add_switches(
                    &mut new_plan,
                    rest_chars,
                    use_as_start,
                    enabled_modes,
                );
            }
            if result.end {
                // Bütün mode'lar bir seferde tek karakter ilerlediğinden bu değer
                // hepsi için ayarlanabilir.
                at_end = true;
            }
            if result.end != at_end {
                return None;
            }
        }

        if at_end {
            // Bütün planlar end of data noktasındadır. Mode'a özgü bitişi
            // geçersiz olan planları dominance pruning uygulanmadan önce eleriz;
            // aksi halde düşük görünen eksik bir C40/Text grubu geçerli ASCII
            // alternatifini kaldırabilir.
            let plan = new_plan
                .into_iter()
                .filter_map(|plan| plan.end_cost().map(|cost| (plan, cost)))
                .min_by_key(|(plan, cost)| {
                    // Eşitliği çözmek için ".index()" sıralamasını kullanırız.
                    let max_enc = plan
                        .switches
                        .iter()
                        .map(|e| e.1.index())
                        .max()
                        .unwrap_or(usize::MAX);
                    (cost.ceil(), max_enc, plan.switches.len())
                })?
                .0;
            return Some(plan);
        }

        remove_hopeless_cases(&mut new_plan);

        if new_plan.is_empty() {
            return None;
        }
        core::mem::swap(&mut plans, &mut new_plan);
    }
    None
}

// Her başlangıç mode'u için yalnızca bir minimizer tutar.
fn remove_hopeless_cases(list: &mut Vec<GenericPlan>) {
    list.sort_unstable_by_key(Plan::cost);

    // Aynı başlangıç mode'u, geçerli mode ve mode-içi artık durumuna sahip
    // planlar arasında yalnızca minimumu tutar. C40/Text ve X12/EDIFACT grup
    // sınırları mode switch geçerliliğini değiştirdiğinden bu durum atılamaz.
    const STATES_PER_MODE: usize = 8;
    let mut seen = [false; 6 * 6 * STATES_PER_MODE];
    let mut unique = Vec::with_capacity(list.len());
    for pl in list.drain(..) {
        let pl_idx =
            (pl.start_mode().index() * 6 + pl.current().index()) * STATES_PER_MODE + pl.state_key();
        if let Some(was_seen) = seen.get_mut(pl_idx)
            && !*was_seen
        {
            *was_seen = true;
            unique.push(pl);
        }
    }
    *list = unique;

    let mut start = 0;
    while start + 1 < list.len() {
        let Some(first) = list.get(start).cloned() else {
            break;
        };
        // `first` planının geçerli mode'unun A olduğunu varsayalım. `first` planının
        // B mode'una geçiş cost'u, geçerli mode'u B olan başka bir plandan küçük
        // veya ona eşitse diğer plan kaldırılabilir.
        let mut uncomparable = false;
        let mut index = start + 1;
        while index < list.len() {
            let Some(second) = list.get(index) else {
                break;
            };
            // Bir mode'a yeni geçiş o mode'un yalnızca sıfır artık durumunu
            // oluşturur. Bu nedenle gerek aynı mode'daki farklı artık durumlar,
            // gerek başka bir mode'dan geçilerek yeniden üretilemeyen sıfır-dışı
            // durumlar birbirinin yerine kullanılamaz.
            let comparable_state = if first.current() == second.current() {
                first.state_key() == second.state_key()
            } else {
                second.state_key() == 0
            };
            if !comparable_state {
                index += 1;
                continue;
            }
            if let Some(first_cost) = first.cost_for_switching_to(second.current()) {
                let second_cost = second.cost();
                if first_cost < second_cost {
                    list.remove(index);
                } else {
                    index += 1;
                }
            } else {
                uncomparable = true;
                break;
            }
        }
        if uncomparable {
            start += 1;
        } else {
            break;
        }
    }
}

#[test]
fn test_hopeless_remove_duplicates() {
    let symbols = crate::SymbolList::default();
    let mut a = GenericPlan::for_mode(EncodationType::Ascii, &[1, 2, 3], 0, &symbols);
    a.step(); // cost = 1
    let mut b = GenericPlan::for_mode(EncodationType::C40, b"ACD", 0, &symbols);
    b.step();
    b.step(); // cost = 4/3
    let mut c = GenericPlan::for_mode(EncodationType::X12, b"ACD", 0, &symbols);
    c.step();
    c.step(); // cost = 4/3
    let mut list = vec![a.clone(), b.clone(), c.clone()];
    remove_hopeless_cases(&mut list);
    assert_eq!(list, vec![a, b, c]);
}

#[test]
fn test_hopeless_remove_1() {
    let symbols = crate::SymbolList::default();
    let a = GenericPlan::for_mode(EncodationType::Ascii, &[1, 2, 3], 0, &symbols);
    let mut b = GenericPlan::for_mode(EncodationType::C40, b"ACD", 0, &symbols);
    b.step();
    b.step();
    b.step();
    let mut list = vec![a.clone(), b];
    remove_hopeless_cases(&mut list);
    assert_eq!(list, vec![a]);
}

#[test]
fn test_hopeless_remove_2() {
    let symbols = SymbolList::default();
    let mut a = GenericPlan::for_mode(EncodationType::Ascii, &[1, 2, 3], 0, &symbols);
    a.step();
    a.step();
    let mut c = GenericPlan::for_mode(EncodationType::C40, b"ABCDEFGH", 0, &symbols);
    c.step(); // Boundary değildir; karşılaştırılmadığı için tutulur.
    let mut list = vec![a.clone(), c.clone()];
    remove_hopeless_cases(&mut list);
    assert_eq!(list, vec![c, a]);
}

#[test]
fn test_hopeless_keeps_cross_mode_residual_state() {
    let symbols = SymbolList::default();
    let ascii = GenericPlan::for_mode(EncodationType::Ascii, b"CD", 0, &symbols);
    let mut c40 = GenericPlan::for_mode(EncodationType::C40, b"ABCD", 0, &symbols);
    c40.step();
    c40.step();

    // ASCII -> C40 geçişi maliyetçe daha ucuz görünse de yeni C40 planı artık
    // durumu 0 ile başlar; iki bekleyen C40 değerini temsil eden planı ezemez.
    let mut list = vec![ascii.clone(), c40.clone()];
    remove_hopeless_cases(&mut list);
    assert_eq!(list, vec![ascii, c40]);
}

#[test]
fn test_ascii_case1() {
    let result = optimize(
        b"ab*de",
        0,
        EncodationType::Ascii,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::Ascii)
    );
}

#[test]
fn test_x12_case1() {
    // Sona kadar X12'ye geçmesi gereken b"ABC>ABC123>ABCDE" verisinden alınmıştır.
    let result = optimize(
        b"BCDE",
        0,
        EncodationType::X12,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::X12)
    );
}

#[test]
fn test_x12_case2() {
    let result = optimize(
        b"CP0*",
        3,
        EncodationType::X12,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::X12)
    );
}

#[test]
fn test_x12_case3() {
    // X12 boyutu: Latch + 3 * 2 + ascii(00) = 8
    // EDIFACT boyutu: Latch + 2 * 3 + UNLATCH + ascii(00) = 9
    let result = optimize(
        b"*********00",
        0,
        EncodationType::Ascii,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::X12)
    );
}

#[test]
fn test_edifact_case1() {
    let result = optimize(
        b"XX",
        42,
        EncodationType::Edifact,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::Edifact)
    );
}

#[test]
fn test_edifact_case2() {
    // Sıradaki karakter EDIFACT ile encode edilemez.
    let result = optimize(
        &[140, 77, 37, 91, 75, 91, 89, 91],
        971,
        EncodationType::Edifact,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert!(result.is_some());
}

#[test]
fn test_x12_case4() {
    let result = optimize(
        b"AIMaimaimaim",
        11,
        EncodationType::X12,
        &SymbolList::default(),
        EncodationType::all(),
    );
    assert_eq!(
        result.and_then(|value| value.first().map(|entry| entry.1)),
        Some(EncodationType::X12)
    );
}
