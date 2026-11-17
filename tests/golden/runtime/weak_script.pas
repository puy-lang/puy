// `[weak]` at the top level of a script (grammar.md 1.1, attributes.md 1.3): after `var`, the
// attributes are skipped to tell an inline variable (`var [weak] W := …`) from a `var` section,
// and a section goes on past a declaration that starts with attributes.

type
  TItem = class(TRefObject)
  public
    Name: string;
  end;

var
  Keep: TItem;
  [weak] First: TItem;
  [weak] Second: TItem;

Keep := TItem.Create;
Keep.Name := 'kept';
First := Keep;
var [weak] Alias := Keep;
var [weak] Typed: TItem := First;
Second := Typed;
WriteLn(Alias.Name, ' ', Second.Name);
