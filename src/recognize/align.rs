use crate::models::Coordinate;

/// 从 Coordinate 提取 y 坐标中心点
pub fn extract_y_centers(coords: &[Coordinate]) -> Vec<i32> {
    coords.iter().map(|c| c.y + c.h / 2).collect()
}

/// 漏检情况：检测数 < 标注数
///
/// 用完整列作为基准，给缺失列的每个点找完整列中y差距最小的点。
/// 完整列中没被匹配到的点的索引就是标注数据要删除的。
///
/// # Arguments
/// * `complete_y` - 完整列的检测 y 坐标（数量 = 标注数量）
/// * `incomplete_y` - 缺失列的检测 y 坐标（数量 < 标注数量）
///
/// # Returns
/// 缺失的标注点索引列表
pub fn find_missing_indices(complete_y: &[i32], incomplete_y: &[i32]) -> Vec<usize> {
    let n = complete_y.len();
    let m = incomplete_y.len();

    if m >= n {
        return Vec::new();
    }

    // 记录完整列中哪些点被匹配了
    let mut matched = vec![false; n];

    // 给缺失列的每个点找完整列中y差距最小的点
    for &inc_y in incomplete_y {
        let mut best_idx = 0;
        let mut best_diff = i32::MAX;

        for (idx, &comp_y) in complete_y.iter().enumerate() {
            if matched[idx] {
                continue; // 已经被匹配过的跳过
            }
            let diff = (comp_y - inc_y).abs();
            if diff < best_diff {
                best_diff = diff;
                best_idx = idx;
            }
        }

        matched[best_idx] = true;
    }

    // 完整列中没被匹配到的索引就是缺失的
    matched.iter()
        .enumerate()
        .filter(|(_, &m)| !m)
        .map(|(i, _)| i)
        .collect()
}

/// 漏检情况（余弦相似度优化版）：检测数 < 标注数
///
/// 通过枚举所有可能的缺失点组合，计算去除这些点后与缺失列的余弦相似度，
/// 选择相似度最高的组合作为缺失点。这种方法对试卷扭曲更鲁棒。
pub fn find_missing_indices_cos(complete_y: &[i32], incomplete_y: &[i32]) -> Vec<usize> {
    let n = complete_y.len();
    let m = incomplete_y.len();

    if m >= n {
        return Vec::new();
    }

    let diff = n - m;
    if diff > 2 {
        return find_missing_indices(complete_y, incomplete_y);
    }

    let mut best_indices = Vec::new();
    let mut best_similarity = f64::NEG_INFINITY;

    // 枚举所有可能的缺失点组合
    generate_combinations(n, diff, &mut vec![], 0, &mut |indices| {
        // 构造去除缺失点后的序列
        let filtered: Vec<i32> = complete_y
            .iter()
            .enumerate()
            .filter(|(i, _)| !indices.contains(i))
            .map(|(_, &y)| y)
            .collect();

        // 计算余弦相似度
        let similarity = cosine_similarity(&filtered, incomplete_y);

        if similarity > best_similarity {
            best_similarity = similarity;
            best_indices = indices.clone();
        }
    });

    best_indices
}

/// 多检情况：检测数 > 标注数
///
/// 用完整列作为基准，给完整列的每个点找多检列中y差距最小的点。
/// 多检列中没被匹配到的点的索引就是多余的检测点。
pub fn find_extra_indices(complete_y: &[i32], extra_y: &[i32]) -> Vec<usize> {
    let n = complete_y.len();
    let m = extra_y.len();

    if m <= n {
        return Vec::new();
    }

    // 记录多检列中哪些点被匹配了
    let mut matched = vec![false; m];

    // 给完整列的每个点找多检列中y差距最小的点
    for &comp_y in complete_y {
        let mut best_idx = 0;
        let mut best_diff = i32::MAX;

        for (idx, &ext_y) in extra_y.iter().enumerate() {
            if matched[idx] {
                continue;
            }
            let diff = (comp_y - ext_y).abs();
            if diff < best_diff {
                best_diff = diff;
                best_idx = idx;
            }
        }

        matched[best_idx] = true;
    }

    // 多检列中没被匹配到的索引就是多余的
    matched.iter()
        .enumerate()
        .filter(|(_, &m)| !m)
        .map(|(i, _)| i)
        .collect()
}

/// 多检情况（余弦相似度优化版）：检测数 > 标注数
pub fn find_extra_indices_cos(complete_y: &[i32], extra_y: &[i32]) -> Vec<usize> {
    let n = complete_y.len();
    let m = extra_y.len();

    if m <= n {
        return Vec::new();
    }

    let diff = m - n;
    if diff > 2 {
        return find_extra_indices(complete_y, extra_y);
    }

    let mut best_indices = Vec::new();
    let mut best_similarity = f64::NEG_INFINITY;

    // 枚举所有可能的多余点组合
    generate_combinations(m, diff, &mut vec![], 0, &mut |indices| {
        // 构造去除多余点后的序列
        let filtered: Vec<i32> = extra_y
            .iter()
            .enumerate()
            .filter(|(i, _)| !indices.contains(i))
            .map(|(_, &y)| y)
            .collect();

        // 计算余弦相似度
        let similarity = cosine_similarity(&filtered, complete_y);

        if similarity > best_similarity {
            best_similarity = similarity;
            best_indices = indices.clone();
        }
    });

    best_indices
}

/// 根据多余索引过滤坐标列表
pub fn filter_by_extra_indices(coords: &[Coordinate], extra_indices: &[usize]) -> Vec<Coordinate> {
    coords
        .iter()
        .enumerate()
        .filter(|(i, _)| !extra_indices.contains(i))
        .map(|(_, c)| c.clone())
        .collect()
}

/// 计算两个向量的余弦相似度
fn cosine_similarity(a: &[i32], b: &[i32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return f64::NEG_INFINITY;
    }

    let dot_product: i64 = a.iter().zip(b.iter()).map(|(&x, &y)| x as i64 * y as i64).sum();
    let norm_a: f64 = (a.iter().map(|&x| (x as i64) * (x as i64)).sum::<i64>() as f64).sqrt();
    let norm_b: f64 = (b.iter().map(|&x| (x as i64) * (x as i64)).sum::<i64>() as f64).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product as f64 / (norm_a * norm_b)
}

/// 生成组合
fn generate_combinations<F>(n: usize, k: usize, current: &mut Vec<usize>, start: usize, callback: &mut F)
where
    F: FnMut(&Vec<usize>),
{
    if current.len() == k {
        callback(current);
        return;
    }

    for i in start..n {
        current.push(i);
        generate_combinations(n, k, current, i + 1, callback);
        current.pop();
    }
}
