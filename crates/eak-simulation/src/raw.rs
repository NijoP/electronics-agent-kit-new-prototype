//! Parser for ngspice ASCII rawfiles (`set filetype=ascii` + `write`). Real data only.

use crate::SimError;

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    /// `voltage`, `current`, `time`, ...
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawPlot {
    pub plotname: String,
    pub variables: Vec<Variable>,
    /// `columns[v][point]`
    pub columns: Vec<Vec<f64>>,
}

impl RawPlot {
    pub fn points(&self) -> usize {
        self.columns.first().map_or(0, Vec::len)
    }
    pub fn column(&self, name: &str) -> Option<&[f64]> {
        self.variables
            .iter()
            .position(|v| v.name.eq_ignore_ascii_case(name))
            .map(|i| self.columns[i].as_slice())
    }
}

fn bad(m: impl Into<String>) -> SimError {
    SimError::BadOutput(m.into())
}

pub fn parse(text: &str) -> Result<RawPlot, SimError> {
    let mut plotname = String::new();
    let (mut nvars, mut npoints) = (0usize, 0usize);
    let mut flags = String::new();
    let mut lines = text.lines();
    let mut variables = Vec::new();
    while let Some(l) = lines.next() {
        let t = l.trim();
        if let Some(v) = t.strip_prefix("Plotname:") {
            plotname = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("Flags:") {
            flags = v.trim().to_ascii_lowercase();
        } else if let Some(v) = t.strip_prefix("No. Variables:") {
            nvars = v.trim().parse().map_err(|_| bad("bad variable count"))?;
        } else if let Some(v) = t.strip_prefix("No. Points:") {
            npoints = v.trim().parse().map_err(|_| bad("bad point count"))?;
        } else if t == "Variables:" {
            for _ in 0..nvars {
                let l = lines.next().ok_or_else(|| bad("truncated variable list"))?;
                let mut it = l.split_whitespace();
                let (_, name, kind) = (it.next(), it.next(), it.next());
                match (name, kind) {
                    (Some(n), Some(k)) => variables.push(Variable {
                        name: n.to_string(),
                        kind: k.to_string(),
                    }),
                    _ => return Err(bad(format!("bad variable line '{l}'"))),
                }
            }
        } else if t == "Values:" {
            break;
        }
    }
    if flags.contains("complex") {
        return Err(bad("complex (AC) rawfiles are not supported"));
    }
    if nvars == 0 || variables.len() != nvars || npoints == 0 {
        return Err(bad("rawfile has no data"));
    }
    let mut toks = lines.flat_map(str::split_whitespace);
    let mut columns = vec![Vec::with_capacity(npoints); nvars];
    for _ in 0..npoints {
        toks.next().ok_or_else(|| bad("truncated values"))?; // point index
        for col in columns.iter_mut() {
            let t = toks.next().ok_or_else(|| bad("truncated values"))?;
            col.push(
                t.parse::<f64>()
                    .map_err(|_| bad(format!("bad number '{t}'")))?,
            );
        }
    }
    Ok(RawPlot {
        plotname,
        variables,
        columns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Title: t\nPlotname: Operating Point\nFlags: real\nNo. Variables: 2\nNo. Points: 1\nVariables:\n\t0\tn_out\tvoltage\n\t1\tv1#branch\tcurrent\nValues:\n 0\t2.5e+00\n\t-1.0e-04\n";

    #[test]
    fn parses_op_plot() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.plotname, "Operating Point");
        assert_eq!(p.column("N_OUT").unwrap(), &[2.5]);
        assert_eq!(p.column("v1#branch").unwrap(), &[-1.0e-4]);
    }

    #[test]
    fn truncated_or_empty_rawfile_is_an_error() {
        assert!(parse("").is_err());
        assert!(parse(&SAMPLE.replace("\t-1.0e-04\n", "")).is_err());
    }
}
