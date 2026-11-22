//! The functions of the 公式 language: names, how many arguments each takes,
//! and the lines the editor's list shows. What they compute is in `eval`.

use super::FnDoc;

/// No upper limit on the arguments.
const ANY: u8 = u8::MAX;

macro_rules! functions {
    ($($group:literal { $($id:ident($min:expr, $max:expr) $sig:literal $desc:literal;)* })*) => {
        /// A function, by its (upper-case) name.
        #[allow(clippy::upper_case_acronyms, clippy::enum_variant_names, non_camel_case_types)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum F { $($($id,)*)* }
        static DOCS: &[FnDoc] = &[$($(FnDoc { name: stringify!($id), group: $group, sig: $sig, desc: $desc },)*)*];
        static ARITY: &[(F, u8, u8)] = &[$($((F::$id, $min, $max),)*)*];
    };
}

functions! {
    "逻辑" {
        IF(2, 3) "IF(条件, 真值, [假值])" "条件成立返回真值，否则返回假值";
        IFS(2, ANY) "IFS(条件1, 值1, [条件2, 值2, …])" "返回第一个成立的条件对应的值";
        SWITCH(3, ANY) "SWITCH(表达式, 匹配1, 值1, …, [默认值])" "表达式等于哪个匹配项就返回它的值，都不等于时返回默认值";
        AND(1, ANY) "AND(条件1, [条件2, …])" "所有条件都成立时为真";
        OR(1, ANY) "OR(条件1, [条件2, …])" "任意一个条件成立时为真";
        XOR(1, ANY) "XOR(条件1, [条件2, …])" "成立的条件有奇数个时为真";
        NOT(1, 1) "NOT(条件)" "条件取反";
        TRUE(0, 0) "TRUE()" "逻辑值真";
        FALSE(0, 0) "FALSE()" "逻辑值假";
        IFERROR(1, 2) "IFERROR(值, [出错时的值])" "值出错时返回备用值，否则返回值本身";
        ISERROR(1, 1) "ISERROR(值)" "值是否为错误";
        ISBLANK(1, 1) "ISBLANK(值)" "值是否为空";
        IFBLANK(2, 2) "IFBLANK(值, 为空时的值)" "值为空时返回备用值，否则返回值本身";
        ISNUMBER(1, 1) "ISNUMBER(值)" "值是否为数字";
        ISTEXT(1, 1) "ISTEXT(值)" "值是否为文本";
    }
    "数字" {
        SUM(1, ANY) "SUM(数字1, [数字2, …])" "求和";
        AVERAGE(1, ANY) "AVERAGE(数字1, [数字2, …])" "平均值";
        MAX(1, ANY) "MAX(数字1, [数字2, …])" "最大值（也可用于日期）";
        MIN(1, ANY) "MIN(数字1, [数字2, …])" "最小值（也可用于日期）";
        MEDIAN(1, ANY) "MEDIAN(数字1, [数字2, …])" "中位数";
        PRODUCT(1, ANY) "PRODUCT(数字1, [数字2, …])" "连乘";
        COUNT(1, ANY) "COUNT(值1, [值2, …])" "数字的个数";
        COUNTA(1, ANY) "COUNTA(值1, [值2, …])" "非空值的个数";
        ABS(1, 1) "ABS(数字)" "绝对值";
        ROUND(1, 2) "ROUND(数字, [小数位数])" "四舍五入到指定小数位";
        ROUNDUP(1, 2) "ROUNDUP(数字, [小数位数])" "向远离 0 的方向进位";
        ROUNDDOWN(1, 2) "ROUNDDOWN(数字, [小数位数])" "向 0 的方向舍去";
        TRUNC(1, 2) "TRUNC(数字, [小数位数])" "截去多余的小数位";
        INT(1, 1) "INT(数字)" "向下取整";
        CEILING(1, 2) "CEILING(数字, [基数])" "向上取到基数的整数倍";
        FLOOR(1, 2) "FLOOR(数字, [基数])" "向下取到基数的整数倍";
        MOD(2, 2) "MOD(被除数, 除数)" "余数，符号与除数相同";
        QUOTIENT(2, 2) "QUOTIENT(被除数, 除数)" "商的整数部分";
        POWER(2, 2) "POWER(底数, 指数)" "乘方";
        SQRT(1, 1) "SQRT(数字)" "平方根";
        EXP(1, 1) "EXP(数字)" "e 的指定次方";
        LN(1, 1) "LN(数字)" "自然对数";
        LOG(1, 2) "LOG(数字, [底数])" "对数，底数默认为 10";
        LOG10(1, 1) "LOG10(数字)" "以 10 为底的对数";
        SIGN(1, 1) "SIGN(数字)" "正数为 1，负数为 -1，零为 0";
        VALUE(1, 1) "VALUE(文本)" "把文本转成数字";
        RAND(0, 0) "RAND()" "0 到 1 之间的随机数";
        RANDBETWEEN(2, 2) "RANDBETWEEN(最小值, 最大值)" "两个数之间的随机整数";
        PI(0, 0) "PI()" "圆周率";
    }
    "文本" {
        CONCATENATE(1, ANY) "CONCATENATE(文本1, [文本2, …])" "把多段文本连接起来";
        CONCAT(1, ANY) "CONCAT(文本1, [文本2, …])" "把多段文本连接起来";
        LEN(1, 1) "LEN(文本)" "文本的字符数";
        LEFT(1, 2) "LEFT(文本, [字符数])" "从开头取若干个字符";
        RIGHT(1, 2) "RIGHT(文本, [字符数])" "从结尾取若干个字符";
        MID(3, 3) "MID(文本, 起始位置, 字符数)" "从指定位置（从 1 起）取若干个字符";
        UPPER(1, 1) "UPPER(文本)" "转成大写";
        LOWER(1, 1) "LOWER(文本)" "转成小写";
        TRIM(1, 1) "TRIM(文本)" "去掉首尾空格和多余的连续空格";
        SUBSTITUTE(3, 4) "SUBSTITUTE(文本, 旧文本, 新文本, [第几次])" "把旧文本替换成新文本，可只替换第几次出现的";
        REPLACE(4, 4) "REPLACE(文本, 起始位置, 字符数, 新文本)" "把指定位置的若干个字符换成新文本";
        FIND(2, 3) "FIND(查找内容, 文本, [起始位置])" "查找内容在文本中的位置，找不到返回 0";
        SEARCH(2, 3) "SEARCH(查找内容, 文本, [起始位置])" "同 FIND，但不区分大小写";
        CONTAINS(2, 2) "CONTAINS(文本, 查找内容)" "文本是否包含查找内容";
        EXACT(2, 2) "EXACT(文本1, 文本2)" "两段文本是否完全相同（区分大小写）";
        REPT(2, 2) "REPT(文本, 次数)" "把文本重复若干次";
        TEXT(2, 2) "TEXT(值, 格式)" "按格式把数字或日期转成文本，如 \"0.00\"、\"#,##0\"、\"0%\"、\"YYYY-MM-DD HH:mm\"";
        T(1, 1) "T(值)" "值是文本时返回它，否则返回空文本";
        ENCODEURL(1, 1) "ENCODEURL(文本)" "把文本编码成可以放进网址的形式";
        SPLIT(2, 2) "SPLIT(文本, 分隔符)" "按分隔符把文本拆成多个值";
        JOIN(2, ANY) "JOIN(分隔符, 值1, [值2, …])" "用分隔符把多个值连成文本，忽略空值";
        TEXTJOIN(2, ANY) "TEXTJOIN(分隔符, 值1, [值2, …])" "用分隔符把多个值连成文本，忽略空值";
        REGEXMATCH(2, 2) "REGEXMATCH(文本, 正则表达式)" "文本是否匹配正则表达式";
        REGEXREPLACE(3, 3) "REGEXREPLACE(文本, 正则表达式, 替换内容)" "把匹配正则表达式的部分换成替换内容";
        REGEXEXTRACT(2, 2) "REGEXEXTRACT(文本, 正则表达式)" "取出第一处匹配正则表达式的内容";
    }
    "日期" {
        TODAY(0, 0) "TODAY()" "今天的日期";
        NOW(0, 0) "NOW()" "现在的日期和时间";
        DATE(3, 5) "DATE(年, 月, 日, [时], [分])" "由年、月、日组成日期";
        YEAR(1, 1) "YEAR(日期)" "日期的年份";
        MONTH(1, 1) "MONTH(日期)" "日期的月份";
        DAY(1, 1) "DAY(日期)" "日期是当月的第几天";
        HOUR(1, 1) "HOUR(日期)" "时间的小时";
        MINUTE(1, 1) "MINUTE(日期)" "时间的分钟";
        SECOND(1, 1) "SECOND(日期)" "时间的秒";
        QUARTER(1, 1) "QUARTER(日期)" "日期在第几季度";
        WEEKDAY(1, 2) "WEEKDAY(日期, [类型])" "星期几：类型 1 周日为 1（默认），类型 2 周一为 1";
        WEEKNUM(1, 2) "WEEKNUM(日期, [类型])" "一年中的第几周：类型 1 每周从周日开始（默认），类型 2 从周一开始";
        DATEDIF(3, 3) "DATEDIF(开始日期, 结束日期, 单位)" "两个日期的间隔，单位为 \"Y\"、\"M\"、\"D\"、\"YM\"、\"MD\" 或 \"YD\"";
        DAYS(2, 2) "DAYS(结束日期, 开始日期)" "两个日期相差的天数";
        DATEADD(3, 3) "DATEADD(日期, 数量, 单位)" "日期加上一段时间，单位为 \"days\"、\"weeks\"、\"months\"、\"years\"、\"hours\" 或 \"minutes\"";
        EDATE(2, 2) "EDATE(日期, 月数)" "若干个月之后（负数为之前）的同一天";
        EOMONTH(2, 2) "EOMONTH(日期, 月数)" "若干个月之后那个月的最后一天";
        WORKDAY(2, 3) "WORKDAY(开始日期, 天数, [节假日])" "若干个工作日之后的日期，跳过周末和节假日";
        NETWORKDAYS(2, 3) "NETWORKDAYS(开始日期, 结束日期, [节假日])" "两个日期之间（含首尾）的工作日天数";
        DATEVALUE(1, 1) "DATEVALUE(文本)" "把文本转成日期";
        TIMESTR(1, 1) "TIMESTR(日期)" "日期的时间部分，如 09:30:00";
        DATESTR(1, 1) "DATESTR(日期)" "日期的日期部分，如 2026-01-30";
        ISTODAY(1, 1) "ISTODAY(日期)" "日期是否是今天";
    }
    "其它" {
        RECORD_ID(0, 0) "RECORD_ID()" "当前记录的 ID";
        CREATED_TIME(0, 0) "CREATED_TIME()" "当前记录的创建时间";
        LAST_MODIFIED_TIME(0, 0) "LAST_MODIFIED_TIME()" "当前记录的最后更新时间";
        ARRAYJOIN(1, 2) "ARRAYJOIN(多个值, [分隔符])" "用分隔符把多个值连成文本";
        ARRAYUNIQUE(1, ANY) "ARRAYUNIQUE(多个值)" "去掉重复的值";
        COUNTIF(2, 2) "COUNTIF(多个值, 条件)" "满足条件的值的个数，条件可以是一个值或 \">5\" 这样的比较";
        FIRST(1, ANY) "FIRST(多个值)" "第一个非空值";
        LAST(1, ANY) "LAST(多个值)" "最后一个非空值";
    }
}

pub(super) fn docs() -> &'static [FnDoc] { DOCS }

/// The function called `name` (any case), with its fewest and most arguments.
pub(super) fn lookup(name: &str) -> Option<(F, usize, usize)> {
    let i = DOCS.iter().position(|d| d.name.eq_ignore_ascii_case(name))?;
    let (f, min, max) = ARITY[i];
    Some((f, min as usize, if max == ANY { usize::MAX } else { max as usize }))
}

/// Why `n` arguments are wrong for `name`, if they are.
pub(super) fn arity_error(name: &str, f: F, min: usize, max: usize, n: usize) -> Option<String> {
    let name = name.to_ascii_uppercase();
    if n < min || n > max {
        return Some(match (min, max) {
            (0, 0) => format!("{name} 不需要参数"),
            _ if min == max => format!("{name} 需要 {min} 个参数"),
            _ if n < min => format!("{name} 至少需要 {min} 个参数"),
            _ => format!("{name} 最多接受 {max} 个参数"),
        });
    }
    (f == F::IFS && n % 2 == 1).then(|| "IFS 的参数要成对出现（条件, 值）".to_string())
}
