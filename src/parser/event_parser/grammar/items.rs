use super::*;

pub(super) fn mod_contents(p: &mut Parser, stop_on_r_curly: bool) {
    attributes::inner_attributes(p);
    while !p.at(EOF) && !(stop_on_r_curly && p.at(R_CURLY)) {
        item(p);
    }
}

pub(super) const ITEM_FIRST: TokenSet =
    token_set![EXTERN_KW, MOD_KW, USE_KW, STRUCT_KW, FN_KW, PUB_KW, POUND,];

fn item(p: &mut Parser) {
    let item = p.start();
    attributes::outer_attributes(p);
    visibility(p);
    let la = p.raw_lookahead(1);
    let item_kind = match p.current() {
        EXTERN_KW if la == CRATE_KW => {
            extern_crate_item(p);
            EXTERN_CRATE_ITEM
        }
        MOD_KW => {
            mod_item(p);
            MOD_ITEM
        }
        USE_KW => {
            use_item(p);
            USE_ITEM
        }
        STRUCT_KW => {
            struct_item(p);
            STRUCT_ITEM
        }
        FN_KW => {
            fn_item(p);
            FN_ITEM
        }
        err_token => {
            item.abandon(p);
            let message = if err_token == SEMI {
                //TODO: if the item is incomplete, this message is misleading
                "expected item, found `;`\n\
                 consider removing this semicolon"
            } else {
                "expected item"
            };
            p.err_and_bump(message);
            return;
        }
    };
    item.complete(p, item_kind);
}

fn struct_item(p: &mut Parser) {
    assert!(p.at(STRUCT_KW));
    p.bump();

    if !p.expect(IDENT) {
        return;
    }
    generic_parameters(p);
    match p.current() {
        WHERE_KW => {
            where_clause(p);
            match p.current() {
                SEMI => {
                    p.bump();
                    return;
                }
                L_CURLY => named_fields(p),
                _ => {
                    //TODO: special case `(` error message
                    p.error().message("expected `;` or `{`").emit();
                    return;
                }
            }
        }
        SEMI => {
            p.bump();
            return;
        }
        L_CURLY => named_fields(p),
        L_PAREN => {
            pos_fields(p);
            p.expect(SEMI);
        }
        _ => {
            p.error().message("expected `;`, `{`, or `(`").emit();
            return;
        }
    }
}

fn named_fields(p: &mut Parser) {
    assert!(p.at(L_CURLY));
    p.bump();
    while !p.at(R_CURLY) && !p.at(EOF) {
        named_field(p);
        if !p.at(R_CURLY) {
            p.expect(COMMA);
        }
    }
    p.expect(R_CURLY);

    fn named_field(p: &mut Parser) {
        let field = p.start();
        visibility(p);
        if p.expect(IDENT) {
            p.expect(COLON);
            types::type_ref(p);
            field.complete(p, NAMED_FIELD);
        } else {
            field.abandon(p);
            p.err_and_bump("expected field declaration");
        }
    }
}

fn pos_fields(p: &mut Parser) {
    if !p.expect(L_PAREN) {
        return;
    }
    while !p.at(R_PAREN) && !p.at(EOF) {
        let pos_field = p.start();
        visibility(p);
        types::type_ref(p);
        pos_field.complete(p, POS_FIELD);

        if !p.at(R_PAREN) {
            p.expect(COMMA);
        }
    }
    p.expect(R_PAREN);
}

fn generic_parameters(_: &mut Parser) {}

fn where_clause(_: &mut Parser) {}

fn extern_crate_item(p: &mut Parser) {
    assert!(p.at(EXTERN_KW));
    p.bump();
    assert!(p.at(CRATE_KW));
    p.bump();

    p.expect(IDENT) && alias(p) && p.expect(SEMI);
}

fn mod_item(p: &mut Parser) {
    assert!(p.at(MOD_KW));
    p.bump();

    if p.expect(IDENT) && !p.eat(SEMI) {
        if p.expect(L_CURLY) {
            mod_contents(p, true);
            p.expect(R_CURLY);
        }
    }
}

pub(super) fn is_use_tree_start(kind: SyntaxKind) -> bool {
    kind == STAR || kind == L_CURLY
}

fn use_item(p: &mut Parser) {
    assert!(p.at(USE_KW));
    p.bump();

    use_tree(p);
    p.expect(SEMI);

    fn use_tree(p: &mut Parser) {
        let la = p.raw_lookahead(1);
        let m = p.start();
        match (p.current(), la) {
            (STAR, _) => {
                p.bump();
            }
            (COLONCOLON, STAR) => {
                p.bump();
                p.bump();
            }
            (L_CURLY, _) | (COLONCOLON, L_CURLY) => {
                if p.at(COLONCOLON) {
                    p.bump();
                }
                nested_trees(p);
            }
            _ if paths::is_path_start(p) => {
                paths::use_path(p);
                match p.current() {
                    AS_KW => {
                        alias(p);
                    }
                    COLONCOLON => {
                        p.bump();
                        match p.current() {
                            STAR => {
                                p.bump();
                            }
                            L_CURLY => nested_trees(p),
                            _ => {
                                // is this unreachable?
                                p.error().message("expected `{` or `*`").emit();
                            }
                        }
                    }
                    _ => (),
                }
            }
            _ => {
                m.abandon(p);
                p.err_and_bump("expected one of `*`, `::`, `{`, `self`, `super`, `indent`");
                return;
            }
        }
        m.complete(p, USE_TREE);
    }

    fn nested_trees(p: &mut Parser) {
        assert!(p.at(L_CURLY));
        p.bump();
        while !p.at(EOF) && !p.at(R_CURLY) {
            use_tree(p);
            if !p.at(R_CURLY) {
                p.expect(COMMA);
            }
        }
        p.expect(R_CURLY);
    }
}

fn fn_item(p: &mut Parser) {
    assert!(p.at(FN_KW));
    p.bump();

    p.expect(IDENT);
    if p.at(L_PAREN) {
        fn_value_parameters(p);
    } else {
        p.error().message("expected function arguments").emit();
    }

    if p.at(L_CURLY) {
        p.expect(L_CURLY);
        p.expect(R_CURLY);
    }

    fn fn_value_parameters(p: &mut Parser) {
        assert!(p.at(L_PAREN));
        p.bump();
        p.expect(R_PAREN);
    }
}
