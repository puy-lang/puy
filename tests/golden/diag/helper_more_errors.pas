program HelperMoreErrors;

// More E0753 of helper declarations (objects.md 10.2, 10.3, 10.9): a destructor, the virtual
// directives other than `virtual`, a method resolution clause, targets that are a class
// reference, a method reference and a method pointer type, a helper as a parameter or result
// type, a `GetEnumerator` for `string`, a set and a static array, and an overload of a member
// the type already has.

type
  IRun = interface
    procedure Run;
  end;

  TBase = class
    procedure Work; virtual;
    procedure Run(const S: string);
  end;

  TBaseClass = class of TBase;
  TRefProc = reference to procedure;
  TMethodProc = procedure of object;

  TBaseHelper = class helper for TBase
    destructor Destroy;                            // E0753: no destructor
    procedure A; dynamic;                          // E0753
    procedure B; virtual; abstract;                // E0753 twice
    procedure Work; override;                      // E0753
    procedure C; reintroduce;                      // E0753
    procedure D; final;                            // E0753
    procedure IRun.Run = Go;                       // E0753: no method resolution clause
    procedure Go;
    procedure Run(N: Integer); overload;           // E0753: adding an overload is the same name
  end;

  TClassRefHelper = class helper for TBaseClass    // E0753: a class reference
  end;

  TRefHelper = record helper for TRefProc          // E0753: a method reference type
  end;

  TMethodHelper = record helper for TMethodProc    // E0753: a method pointer type
  end;

  TTextHelper = record helper for string
    function GetEnumerator: TBase;                 // E0753: `for..in` iterates over strings
  end;

  TDigits = set of 0..9;
  TTriple = array[0..2] of Integer;

  TDigitsHelper = record helper for TDigits
    function GetEnumerator: TBase;                 // E0753: and over sets
  end;

  TTripleHelper = record helper for TTriple
    function GetEnumerator: TBase;                 // E0753: and over static arrays
  end;

procedure TBase.Work;
begin
end;

procedure TBase.Run(const S: string);
begin
end;

procedure TBaseHelper.Go;
begin
end;

function TTextHelper.GetEnumerator: TBase;
begin
  Result := nil;
end;

function TDigitsHelper.GetEnumerator: TBase;
begin
  Result := nil;
end;

function TTripleHelper.GetEnumerator: TBase;
begin
  Result := nil;
end;

procedure Take(H: TTextHelper);                    // E0753: not a type
begin
end;

type
  TGive = function: TTextHelper;                   // E0753: not a type

begin
end.
